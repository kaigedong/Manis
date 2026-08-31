import CryptoKit
import Foundation
import Darwin
import Security

private let serviceName = "dev.manis.app.helper"
private let helperProtocolVersion = "v7"
private let requiredClientRequirement =
    ProcessInfo.processInfo.environment["MANIS_REQUIRED_CLIENT_REQUIREMENT"] ?? ""
private let administratorInstall =
    ProcessInfo.processInfo.environment["MANIS_ADMINISTRATOR_INSTALL"] == "1"
private let allowedLocalUserIdentifier =
    ProcessInfo.processInfo.environment["MANIS_LOCAL_ALLOWED_UID"].flatMap(uid_t.init)
private let managedMihomoPath = "/Library/Application Support/Manis/bin/mihomo"
private let logPath = "/var/log/manis-mihomo-helper.log"
private let maximumConfigBytes = 16 * 1024 * 1024
private let maximumGeodataBytes: off_t = 128 * 1024 * 1024
private let maximumCoreLogBytes: off_t = 4 * 1024 * 1024
private let maximumCoreBytes = 128 * 1024 * 1024
private let coreLogName = "manis-privileged-core.log"
private let optionalGeodataNames = ["geoip.metadb", "geoip.dat", "geosite.dat", "GeoLite2-ASN.mmdb"]

@objc(ManisPrivilegedHelperProtocol)
protocol ManisPrivilegedHelperProtocol {
    func status(withReply reply: @escaping (String, Int32) -> Void)
    func start(
        dataDir: String,
        config: String,
        controller: String,
        withReply reply: @escaping (String, Int32) -> Void
    )
    func stop(withReply reply: @escaping (String, Int32) -> Void)
    func stageCore(
        contents: Data,
        sha256: String,
        withReply reply: @escaping (String, Int32) -> Void
    )
}

final class HelperDelegate: NSObject, NSXPCListenerDelegate {
    private let core = HelperCore()

    func listener(
        _ listener: NSXPCListener,
        shouldAcceptNewConnection connection: NSXPCConnection
    ) -> Bool {
        if administratorInstall {
            guard let allowedLocalUserIdentifier,
                connection.effectiveUserIdentifier == allowedLocalUserIdentifier
            else {
                appendLog("rejected local helper client uid \(connection.effectiveUserIdentifier)")
                return false
            }
        }
        connection.exportedInterface = NSXPCInterface(with: ManisPrivilegedHelperProtocol.self)
        connection.exportedObject = HelperService(
            core: core,
            clientUserIdentifier: connection.effectiveUserIdentifier
        )
        connection.resume()
        return true
    }
}

final class HelperService: NSObject, ManisPrivilegedHelperProtocol {
    private let core: HelperCore
    private let clientUserIdentifier: uid_t

    init(core: HelperCore, clientUserIdentifier: uid_t) {
        self.core = core
        self.clientUserIdentifier = clientUserIdentifier
    }

    func status(withReply reply: @escaping (String, Int32) -> Void) {
        core.status(owner: clientUserIdentifier, withReply: reply)
    }

    func start(
        dataDir: String,
        config: String,
        controller: String,
        withReply reply: @escaping (String, Int32) -> Void
    ) {
        core.start(
            dataDir: dataDir,
            config: config,
            controller: controller,
            owner: clientUserIdentifier,
            withReply: reply
        )
    }

    func stop(withReply reply: @escaping (String, Int32) -> Void) {
        core.stop(owner: clientUserIdentifier, withReply: reply)
    }

    func stageCore(
        contents: Data,
        sha256: String,
        withReply reply: @escaping (String, Int32) -> Void
    ) {
        core.stageCore(contents: contents, sha256: sha256, owner: clientUserIdentifier, withReply: reply)
    }
}

final class HelperCore {
    private let lock = NSLock()
    private var child: Process?
    private var childOwner: uid_t?
    private var stagedConfig: String?
    private var lastExitReason = "not-started"

    func status(owner: uid_t, withReply reply: @escaping (String, Int32) -> Void) {
        lock.withLock {
            reapExitedChild()
            if let process = child, process.isRunning {
                guard childOwner == owner else {
                    reply("stopped \(helperProtocolVersion) \(lastExitReason)", 0)
                    return
                }
                reply("running \(process.processIdentifier) \(helperProtocolVersion)", 0)
            } else {
                child = nil
                childOwner = nil
                reply("stopped \(helperProtocolVersion) \(lastExitReason)", 0)
            }
        }
    }

    func start(
        dataDir: String,
        config: String,
        controller: String,
        owner: uid_t,
        withReply reply: @escaping (String, Int32) -> Void
    ) {
        lock.withLock {
            var candidate: StagedRuntime?
            var coreLog: FileHandle?
            do {
                reapExitedChild()
                if let process = child, process.isRunning {
                    guard childOwner == owner else {
                        reply("error Mihomo is owned by another user", 1)
                        return
                    }
                    reply("started \(process.processIdentifier)", 0)
                    return
                }
                let request = try LaunchRequest(
                    dataDir: dataDir,
                    config: config,
                    controller: controller
                )
                let mihomoBinary = try bundledMihomoPath()
                try validateExecutable(mihomoBinary)
                try validateRuntime(request, owner: owner)
                coreLog = try openCoreLogHandle(dataDir: request.dataDir, owner: owner)
                writeCoreLogMarker(coreLog, message: "helper start requested")
                let staged = try stageRuntime(request, owner: owner)
                candidate = staged
                try validateStagedRuntime(
                    staged,
                    mihomoBinary: mihomoBinary,
                    output: coreLog ?? FileHandle.nullDevice
                )

                let process = Process()
                process.executableURL = URL(fileURLWithPath: mihomoBinary)
                process.currentDirectoryURL = URL(fileURLWithPath: staged.dataDir)
                process.arguments = [
                    "-d", staged.dataDir,
                    "-f", staged.config,
                    "-ext-ctl-unix", request.controller,
                ]
                process.environment = [:]
                process.standardOutput = coreLog ?? FileHandle.nullDevice
                process.standardError = coreLog ?? FileHandle.nullDevice
                process.standardInput = FileHandle.nullDevice
                try process.run()
                try? coreLog?.close()
                coreLog = nil
                child = process
                childOwner = owner
                stagedConfig = staged.config
                lastExitReason = "running"
                candidate = nil
                appendLog("started mihomo pid \(process.processIdentifier)")
                reply("started \(process.processIdentifier)", 0)
            } catch {
                try? coreLog?.close()
                if let candidate {
                    try? FileManager.default.removeItem(atPath: candidate.config)
                }
                lastExitReason = "start-failed"
                appendLog("start failed: \(error)")
                reply("error \(error)", 1)
            }
        }
    }

    func stop(owner: uid_t, withReply reply: @escaping (String, Int32) -> Void) {
        lock.withLock {
            if let process = child, process.isRunning {
                guard childOwner == owner else {
                    reply("error Mihomo is owned by another user", 1)
                    return
                }
                process.terminate()
                let deadline = Date().addingTimeInterval(5)
                while process.isRunning && Date() < deadline {
                    Thread.sleep(forTimeInterval: 0.05)
                }
                var forced = false
                if process.isRunning {
                    forced = true
                    kill(process.processIdentifier, SIGKILL)
                    process.waitUntilExit()
                }
                lastExitReason = describeExit(process, requested: true, forced: forced)
                appendLog(
                    "stopping mihomo pid \(process.processIdentifier) reason \(lastExitReason)"
                )
            }
            child = nil
            childOwner = nil
            removeStagedConfig()
            appendLog("stopped mihomo")
            reply("stopped", 0)
        }
    }

    func stageCore(
        contents: Data,
        sha256 expectedDigest: String,
        owner: uid_t,
        withReply reply: @escaping (String, Int32) -> Void
    ) {
        lock.withLock {
            do {
                reapExitedChild()
                guard child == nil else {
                    throw HelperError.invalidExecutable("stop Mihomo before replacing its core")
                }
                guard contents.count > 0, contents.count <= maximumCoreBytes else {
                    throw HelperError.invalidExecutable("managed Mihomo has an invalid size")
                }
                let actualDigest = SHA256.hash(data: contents)
                    .map { String(format: "%02x", $0) }
                    .joined()
                guard expectedDigest.count == 64,
                    expectedDigest.allSatisfy({ $0.isHexDigit && !$0.isUppercase }),
                    actualDigest == expectedDigest
                else {
                    throw HelperError.invalidExecutable("managed Mihomo digest does not match")
                }
                if administratorInstall {
                    guard try administratorCoreDigestIsTrusted(actualDigest) else {
                        throw HelperError.invalidExecutable("Mihomo is not the approved seed, installed core, or official latest release")
                    }
                } else {
                    try validateContainingBundleSeal()
                }
                try installManagedCore(contents)
                appendLog("staged managed mihomo for uid \(owner) sha256 \(actualDigest)")
                reply("staged \(actualDigest)", 0)
            } catch {
                appendLog("core staging failed: \(error)")
                reply("error \(error)", 1)
            }
        }
    }

    private func reapExitedChild() {
        if let process = child, !process.isRunning {
            lastExitReason = describeExit(process, requested: false, forced: false)
            appendLog(
                "mihomo exited pid \(process.processIdentifier) reason \(lastExitReason)"
            )
            child = nil
            childOwner = nil
            removeStagedConfig()
        }
    }

    private func removeStagedConfig() {
        if let stagedConfig {
            try? FileManager.default.removeItem(atPath: stagedConfig)
        }
        stagedConfig = nil
    }
}

private func bundledMihomoPath() throws -> String {
    return managedMihomoPath
}

private func administratorCoreDigestIsTrusted(_ digest: String) throws -> Bool {
    let approval = try ManisHelperSecurity.Approval(environment: ProcessInfo.processInfo.environment)
    if digest == approval.seedSHA256 { return true }
    // A client-supplied digest is only an integrity check, never authorization to execute code
    // as root. Independently verify provenance, including after an in-app Mihomo update.
    try ManisHelperSecurity.requireRootOwnedPath(managedMihomoPath, directory: false)
    let installed = try Data(contentsOf: URL(fileURLWithPath: managedMihomoPath))
    guard installed.count <= maximumCoreBytes else {
        throw HelperError.invalidExecutable("installed Mihomo exceeds the safety limit")
    }
    let installedDigest = SHA256.hash(data: installed).map { String(format: "%02x", $0) }.joined()
    if digest == installedDigest { return true }
    return try digest == MihomoReleaseVerifier.latestDigest()
}

private func bundleContentsURL() throws -> URL {
    var code: SecCode?
    guard SecCodeCopySelf([], &code) == errSecSuccess, let code else {
        throw HelperError.invalidExecutable("could not inspect the running helper")
    }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode else {
        throw HelperError.invalidExecutable("could not inspect the helper executable")
    }
    var executable: CFURL?
    guard SecCodeCopyPath(staticCode, [], &executable) == errSecSuccess, let executable else {
        throw HelperError.invalidExecutable("could not resolve the running helper path")
    }
    return (executable as URL).resolvingSymlinksInPath()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
}

private func bundleRootURL() throws -> URL {
    try bundleContentsURL().deletingLastPathComponent()
}

private struct LaunchRequest {
    let dataDir: String
    let config: String
    let controller: String

    init(dataDir: String, config: String, controller: String) throws {
        self.dataDir = try cleanAbsolutePath(dataDir, label: "data-dir")
        self.config = try cleanAbsolutePath(config, label: "config")
        self.controller = try cleanAbsolutePath(controller, label: "controller")
    }
}

private struct StagedRuntime {
    let dataDir: String
    let config: String
}

private enum HelperError: Error, CustomStringConvertible {
    case invalidPath(String)
    case invalidRuntime(String)
    case invalidExecutable(String)

    var description: String {
        switch self {
        case .invalidPath(let message),
            .invalidRuntime(let message),
            .invalidExecutable(let message):
            return message
        }
    }
}

private func cleanAbsolutePath(_ path: String, label: String) throws -> String {
    guard path.hasPrefix("/") else {
        throw HelperError.invalidPath("\(label) must be absolute")
    }
    let parts = path.split(separator: "/", omittingEmptySubsequences: true)
    guard !parts.contains(".") && !parts.contains("..") else {
        throw HelperError.invalidPath("\(label) must not contain . or ..")
    }
    return URL(fileURLWithPath: path).standardizedFileURL.path
}

private func validateRuntime(_ request: LaunchRequest, owner: uid_t) throws {
    let dataDir = URL(fileURLWithPath: request.dataDir)
    let config = URL(fileURLWithPath: request.config)
    let controller = URL(fileURLWithPath: request.controller)
    guard config.deletingLastPathComponent().path == dataDir.path else {
        throw HelperError.invalidRuntime("config must be inside the Manis Mihomo runtime")
    }
    guard controller.deletingLastPathComponent().path == dataDir.path,
        controller.lastPathComponent == "controller.sock"
    else {
        throw HelperError.invalidRuntime("controller must be the Manis runtime socket")
    }
    guard config.lastPathComponent == "manis-generated.yaml" else {
        throw HelperError.invalidRuntime("config basename is not allowed")
    }
    let parts = dataDir.path.split(separator: "/", omittingEmptySubsequences: true)
    guard parts.count == 6,
        parts[0] == "Users",
        parts[2] == "Library",
        parts[3] == "Application Support",
        parts[4] == "Manis",
        parts[5] == "mihomo"
    else {
        throw HelperError.invalidRuntime("data-dir must be the Manis user Mihomo runtime")
    }
    try requireDirectory(dataDir.path, owner: owner)
    try requireRegularFile(config.path, owner: owner)
}

private func validateExecutable(_ path: String) throws {
    guard path == (try bundledMihomoPath()) else {
        throw HelperError.invalidExecutable("privileged Mihomo binary must stay in Manis storage")
    }
    try requireRegularFile(path, owner: 0)
    guard FileManager.default.isExecutableFile(atPath: path) else {
        throw HelperError.invalidExecutable("privileged Mihomo binary is not executable")
    }
}

private func installManagedCore(_ contents: Data) throws {
    let directory = URL(fileURLWithPath: managedMihomoPath).deletingLastPathComponent().path
    try createRootOwnedCoreDirectory(directory)
    if FileManager.default.fileExists(atPath: managedMihomoPath) {
        try rejectSymlink(managedMihomoPath)
    }
    let temporary = "\(managedMihomoPath).\(UUID().uuidString).tmp"
    let descriptor = open(temporary, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0o700)
    guard descriptor >= 0 else {
        throw HelperError.invalidExecutable("could not create a staged Mihomo core")
    }
    defer {
        close(descriptor)
        unlink(temporary)
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
    try handle.write(contentsOf: contents)
    try handle.synchronize()
    guard fchown(descriptor, 0, 0) == 0, fchmod(descriptor, 0o755) == 0 else {
        throw HelperError.invalidExecutable("could not secure the staged Mihomo core")
    }
    guard rename(temporary, managedMihomoPath) == 0 else {
        throw HelperError.invalidExecutable("could not publish the staged Mihomo core")
    }
}

private func createRootOwnedCoreDirectory(_ path: String) throws {
    let boundary = "/Library/Application Support/Manis"
    if !FileManager.default.fileExists(atPath: boundary) {
        try FileManager.default.createDirectory(
            atPath: boundary,
            withIntermediateDirectories: false,
            attributes: [
                .ownerAccountID: 0,
                .groupOwnerAccountID: 0,
                .posixPermissions: 0o755,
            ]
        )
    }
    try requireDirectory(boundary, owner: 0)
    if FileManager.default.fileExists(atPath: path) {
        try rejectSymlink(path)
    } else {
        try FileManager.default.createDirectory(
            atPath: path,
            withIntermediateDirectories: false,
            attributes: [
                .ownerAccountID: 0,
                .groupOwnerAccountID: 0,
                .posixPermissions: 0o700,
            ]
        )
    }
    try requireDirectory(path, owner: 0)
    guard chmod(path, 0o700) == 0 else {
        throw HelperError.invalidExecutable("could not secure the Mihomo core directory")
    }
}

private func stageRuntime(_ request: LaunchRequest, owner: uid_t) throws -> StagedRuntime {
    let descriptor = open(request.config, O_RDONLY | O_NOFOLLOW)
    guard descriptor >= 0 else {
        throw HelperError.invalidRuntime("could not open config safely")
    }
    defer { close(descriptor) }

    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
        metadata.st_uid == owner,
        metadata.st_mode & S_IFMT == S_IFREG,
        metadata.st_mode & 0o077 == 0,
        metadata.st_size >= 0,
        metadata.st_size <= maximumConfigBytes
    else {
        throw HelperError.invalidRuntime("config ownership, mode, type, or size is unsafe")
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
    let contents = try handle.readToEnd() ?? Data()
    guard contents.count == metadata.st_size else {
        throw HelperError.invalidRuntime("config changed while it was being staged")
    }

    let root = "/Library/Application Support/Manis/runtime/\(owner)/mihomo"
    try createRootOwnedDirectory(root)
    let config = URL(fileURLWithPath: root).appendingPathComponent("manis-generated.yaml")
    try contents.write(to: config, options: .atomic)
    try FileManager.default.setAttributes(
        [.ownerAccountID: 0, .groupOwnerAccountID: 0, .posixPermissions: 0o600],
        ofItemAtPath: config.path
    )
    for name in optionalGeodataNames {
        try stageOptionalGeodata(
            sourceDirectory: request.dataDir,
            destinationDirectory: root,
            name: name,
            owner: owner
        )
    }
    return StagedRuntime(dataDir: root, config: config.path)
}

private func stageOptionalGeodata(
    sourceDirectory: String,
    destinationDirectory: String,
    name: String,
    owner: uid_t
) throws {
    let source = URL(fileURLWithPath: sourceDirectory).appendingPathComponent(name).path
    let descriptor = open(source, O_RDONLY | O_NOFOLLOW)
    if descriptor < 0 && errno == ENOENT {
        return
    }
    guard descriptor >= 0 else {
        throw HelperError.invalidRuntime("could not open optional geodata safely")
    }
    defer { close(descriptor) }

    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
        metadata.st_uid == owner,
        metadata.st_mode & S_IFMT == S_IFREG,
        metadata.st_size > 0,
        metadata.st_size <= maximumGeodataBytes
    else {
        throw HelperError.invalidRuntime("optional geodata ownership, type, or size is unsafe")
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
    let contents = try handle.readToEnd() ?? Data()
    guard contents.count == metadata.st_size else {
        throw HelperError.invalidRuntime("optional geodata changed while it was being staged")
    }

    let destination = URL(fileURLWithPath: destinationDirectory).appendingPathComponent(name)
    try contents.write(to: destination, options: .atomic)
    try FileManager.default.setAttributes(
        [.ownerAccountID: 0, .groupOwnerAccountID: 0, .posixPermissions: 0o644],
        ofItemAtPath: destination.path
    )
}

private func validateStagedRuntime(
    _ runtime: StagedRuntime,
    mihomoBinary: String,
    output: FileHandle
) throws {
    let validation = Process()
    validation.executableURL = URL(fileURLWithPath: mihomoBinary)
    validation.currentDirectoryURL = URL(fileURLWithPath: runtime.dataDir)
    validation.arguments = ["-t", "-d", runtime.dataDir, "-f", runtime.config]
    validation.environment = [:]
    validation.standardInput = FileHandle.nullDevice
    validation.standardOutput = output
    validation.standardError = output
    try validation.run()

    let deadline = Date().addingTimeInterval(10)
    while validation.isRunning && Date() < deadline {
        Thread.sleep(forTimeInterval: 0.02)
    }
    if validation.isRunning {
        kill(validation.processIdentifier, SIGKILL)
        validation.waitUntilExit()
        throw HelperError.invalidRuntime("staged config validation timed out")
    }
    guard validation.terminationStatus == 0 else {
        throw HelperError.invalidRuntime("Mihomo rejected the staged config")
    }
}

private func openCoreLogHandle(dataDir: String, owner: uid_t) throws -> FileHandle {
    let path = URL(fileURLWithPath: dataDir).appendingPathComponent(coreLogName).path
    let descriptor = open(path, O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW, 0o600)
    guard descriptor >= 0 else {
        throw HelperError.invalidRuntime("could not open privileged Mihomo log safely")
    }
    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
        (metadata.st_uid == owner || metadata.st_uid == 0),
        metadata.st_mode & S_IFMT == S_IFREG
    else {
        close(descriptor)
        throw HelperError.invalidRuntime("privileged Mihomo log ownership or type is unsafe")
    }
    guard fchown(descriptor, owner, gid_t(bitPattern: Int32(-1))) == 0 else {
        close(descriptor)
        throw HelperError.invalidRuntime("could not assign privileged Mihomo log ownership")
    }
    guard fchmod(descriptor, 0o600) == 0 else {
        close(descriptor)
        throw HelperError.invalidRuntime("could not secure privileged Mihomo log")
    }
    if metadata.st_size > maximumCoreLogBytes {
        guard ftruncate(descriptor, 0) == 0 else {
            close(descriptor)
            throw HelperError.invalidRuntime("could not rotate privileged Mihomo log")
        }
    }
    return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
}

private func writeCoreLogMarker(_ handle: FileHandle?, message: String) {
    guard let handle,
        let data = "\n--- \(Date()) \(message) ---\n".data(using: .utf8)
    else {
        return
    }
    try? handle.write(contentsOf: data)
}

private func describeExit(_ process: Process, requested: Bool, forced: Bool) -> String {
    let prefix = requested ? "requested" : "unexpected"
    if forced {
        return "\(prefix)-forced-signal-9"
    }
    switch process.terminationReason {
    case .exit:
        return "\(prefix)-exit-\(process.terminationStatus)"
    case .uncaughtSignal:
        return "\(prefix)-signal-\(process.terminationStatus)"
    @unknown default:
        return "\(prefix)-unknown-\(process.terminationStatus)"
    }
}

private func createRootOwnedDirectory(_ path: String) throws {
    let boundary = "/Library/Application Support/Manis"
    if !FileManager.default.fileExists(atPath: boundary) {
        try FileManager.default.createDirectory(
            atPath: boundary,
            withIntermediateDirectories: false,
            attributes: [
                .ownerAccountID: 0,
                .groupOwnerAccountID: 0,
                .posixPermissions: 0o755,
            ]
        )
    }
    try requireDirectory(boundary, owner: 0)
    try FileManager.default.createDirectory(
        atPath: path,
        withIntermediateDirectories: true,
        attributes: [.posixPermissions: 0o700]
    )
    var current = URL(fileURLWithPath: "/Library/Application Support/Manis")
    let leafComponents = ["runtime"] + path.split(separator: "/").suffix(2).map(String.init)
    for component in leafComponents {
        current.appendPathComponent(component)
        try rejectSymlink(current.path)
        let attributes = try FileManager.default.attributesOfItem(atPath: current.path)
        guard let account = attributes[.ownerAccountID] as? NSNumber,
            account.uint32Value == 0
        else {
            throw HelperError.invalidRuntime("privileged runtime is not root-owned")
        }
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: current.path)
    }
}

private func validateContainingBundleSeal() throws {
    var code: SecStaticCode?
    let createStatus = SecStaticCodeCreateWithPath(try bundleRootURL() as CFURL, [], &code)
    guard createStatus == errSecSuccess, let code else {
        throw HelperError.invalidExecutable("Manis.app code signature is unavailable")
    }
    let checkStatus = SecStaticCodeCheckValidity(
        code,
        SecCSFlags(rawValue: kSecCSStrictValidate),
        nil
    )
    guard checkStatus == errSecSuccess else {
        throw HelperError.invalidExecutable("Manis.app code signature does not seal Mihomo")
    }
}

private func requireDirectory(_ path: String, owner: uid_t?) throws {
    var isDirectory = ObjCBool(false)
    guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), isDirectory.boolValue
    else {
        throw HelperError.invalidRuntime("required runtime directory is missing")
    }
    try rejectSymlink(path)
    try rejectSymlinkEscape(path)
    try requireSafeOwnerAndMode(path, owner: owner)
}

private func requireRegularFile(_ path: String, owner: uid_t?) throws {
    var isDirectory = ObjCBool(false)
    guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory), !isDirectory.boolValue
    else {
        throw HelperError.invalidRuntime("required runtime file is missing")
    }
    try rejectSymlink(path)
    try rejectSymlinkEscape(path)
    try requireSafeOwnerAndMode(path, owner: owner)
}

private func rejectSymlink(_ path: String) throws {
    let values = try URL(fileURLWithPath: path).resourceValues(forKeys: [.isSymbolicLinkKey])
    if values.isSymbolicLink == true {
        throw HelperError.invalidPath("runtime path must not be a symlink")
    }
}

private func rejectSymlinkEscape(_ path: String) throws {
    let url = URL(fileURLWithPath: path).standardizedFileURL
    if url.resolvingSymlinksInPath().path != url.path {
        throw HelperError.invalidPath("runtime path must not contain symlinked parents")
    }
}

private func requireSafeOwnerAndMode(_ path: String, owner: uid_t?) throws {
    let attributes = try FileManager.default.attributesOfItem(atPath: path)
    if let owner {
        guard let account = attributes[.ownerAccountID] as? NSNumber,
            account.uint32Value == owner
        else {
            throw HelperError.invalidRuntime("runtime path owner does not match the client")
        }
    }
    guard let permissions = attributes[.posixPermissions] as? NSNumber else {
        throw HelperError.invalidRuntime("runtime path permissions are unavailable")
    }
    if permissions.uint16Value & 0o022 != 0 {
        throw HelperError.invalidRuntime("runtime path must not be group/world writable")
    }
}

private func openLogHandle() -> FileHandle {
    if !FileManager.default.fileExists(atPath: logPath) {
        FileManager.default.createFile(atPath: logPath, contents: nil)
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: logPath)
    }
    guard let handle = FileHandle(forWritingAtPath: logPath) else {
        return FileHandle.nullDevice
    }
    do {
        try handle.seekToEnd()
    } catch {
        return FileHandle.nullDevice
    }
    return handle
}

private func appendLog(_ message: String) {
    guard let data = "\(Date()) \(message)\n".data(using: .utf8) else {
        return
    }
    let handle = openLogHandle()
    handle.write(data)
    try? handle.close()
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}

let listener = NSXPCListener(machServiceName: serviceName)
let delegate = HelperDelegate()
if requiredClientRequirement.isEmpty {
    appendLog("MANIS_REQUIRED_CLIENT_REQUIREMENT is not configured")
    Foundation.exit(1)
}
if administratorInstall {
    do {
        _ = try ManisHelperSecurity.Approval(environment: ProcessInfo.processInfo.environment)
    } catch {
        appendLog("invalid administrator approval: \(error)")
        Foundation.exit(1)
    }
} else if !requiredClientRequirement.contains("anchor apple generic")
    || !requiredClientRequirement.contains("certificate leaf[subject.OU]")
    || !requiredClientRequirement.contains("identifier \"dev.manis.app.helperctl\"")
{
    appendLog("client code-signing requirement is not production-grade")
    Foundation.exit(1)
}
listener.setConnectionCodeSigningRequirement(requiredClientRequirement)
listener.delegate = delegate
listener.resume()
RunLoop.current.run()

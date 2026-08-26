import Foundation
import Darwin
import Security

private let serviceName = "dev.relay.prototype.helper"
private let helperProtocolVersion = "v2"
private let requiredClientRequirement =
    ProcessInfo.processInfo.environment["RELAY_REQUIRED_CLIENT_REQUIREMENT"] ?? ""
private let allowInsecureLocalRequirement =
    ProcessInfo.processInfo.environment["RELAY_ALLOW_INSECURE_LOCAL_HELPER"] == "1"
private let logPath = "/var/log/relay-mihomo-helper.log"
private let maximumConfigBytes = 16 * 1024 * 1024

@objc(RelayPrivilegedHelperProtocol)
protocol RelayPrivilegedHelperProtocol {
    func status(withReply reply: @escaping (String, Int32) -> Void)
    func start(
        dataDir: String,
        config: String,
        controller: String,
        withReply reply: @escaping (String, Int32) -> Void
    )
    func stop(withReply reply: @escaping (String, Int32) -> Void)
}

final class HelperDelegate: NSObject, NSXPCListenerDelegate {
    private let core = HelperCore()

    func listener(
        _ listener: NSXPCListener,
        shouldAcceptNewConnection connection: NSXPCConnection
    ) -> Bool {
        connection.exportedInterface = NSXPCInterface(with: RelayPrivilegedHelperProtocol.self)
        connection.exportedObject = HelperService(
            core: core,
            clientUserIdentifier: connection.effectiveUserIdentifier
        )
        connection.resume()
        return true
    }
}

final class HelperService: NSObject, RelayPrivilegedHelperProtocol {
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
}

final class HelperCore {
    private let lock = NSLock()
    private var child: Process?
    private var childOwner: uid_t?
    private var stagedConfig: String?

    func status(owner: uid_t, withReply reply: @escaping (String, Int32) -> Void) {
        lock.withLock {
            reapExitedChild()
            if let process = child, process.isRunning {
                guard childOwner == owner else {
                    reply("stopped \(helperProtocolVersion)", 0)
                    return
                }
                reply("running \(process.processIdentifier) \(helperProtocolVersion)", 0)
            } else {
                child = nil
                childOwner = nil
                reply("stopped \(helperProtocolVersion)", 0)
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
                let staged = try stageRuntime(request, owner: owner)
                candidate = staged
                try validateStagedRuntime(staged, mihomoBinary: mihomoBinary)

                let process = Process()
                process.executableURL = URL(fileURLWithPath: mihomoBinary)
                process.currentDirectoryURL = URL(fileURLWithPath: staged.dataDir)
                process.arguments = [
                    "-d", staged.dataDir,
                    "-f", staged.config,
                    "-ext-ctl-unix", request.controller,
                ]
                process.environment = [:]
                process.standardOutput = FileHandle.nullDevice
                process.standardError = FileHandle.nullDevice
                process.standardInput = FileHandle.nullDevice
                try process.run()
                child = process
                childOwner = owner
                stagedConfig = staged.config
                candidate = nil
                appendLog("started mihomo pid \(process.processIdentifier)")
                reply("started \(process.processIdentifier)", 0)
            } catch {
                if let candidate {
                    try? FileManager.default.removeItem(atPath: candidate.config)
                }
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
                if process.isRunning {
                    kill(process.processIdentifier, SIGKILL)
                }
            }
            child = nil
            childOwner = nil
            removeStagedConfig()
            appendLog("stopped mihomo")
            reply("stopped", 0)
        }
    }

    private func reapExitedChild() {
        if let process = child, !process.isRunning {
            appendLog("mihomo exited pid \(process.processIdentifier)")
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
    try bundleContentsURL()
        .appendingPathComponent("Resources")
        .appendingPathComponent("mihomo")
        .appendingPathComponent("mihomo")
        .path
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
        throw HelperError.invalidRuntime("config must be inside the Relay Mihomo runtime")
    }
    guard controller.deletingLastPathComponent().path == dataDir.path,
        controller.lastPathComponent == "controller.sock"
    else {
        throw HelperError.invalidRuntime("controller must be the Relay runtime socket")
    }
    guard config.lastPathComponent == "relay-generated.yaml" else {
        throw HelperError.invalidRuntime("config basename is not allowed")
    }
    let parts = dataDir.path.split(separator: "/", omittingEmptySubsequences: true)
    guard parts.count == 6,
        parts[0] == "Users",
        parts[2] == "Library",
        parts[3] == "Application Support",
        parts[4] == "Relay",
        parts[5] == "mihomo"
    else {
        throw HelperError.invalidRuntime("data-dir must be the Relay user Mihomo runtime")
    }
    try requireDirectory(dataDir.path, owner: owner)
    try requireRegularFile(config.path, owner: owner)
}

private func validateExecutable(_ path: String) throws {
    guard path == (try bundledMihomoPath()) else {
        throw HelperError.invalidExecutable("privileged Mihomo binary must stay inside Relay.app")
    }
    try validateContainingBundleSeal()
    try requireRegularFile(path, owner: nil)
    guard FileManager.default.isExecutableFile(atPath: path) else {
        throw HelperError.invalidExecutable("privileged Mihomo binary is not executable")
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

    let root = "/Library/Application Support/Relay/runtime/\(owner)/mihomo"
    try createRootOwnedDirectory(root)
    let config = URL(fileURLWithPath: root).appendingPathComponent("relay-generated.yaml")
    try contents.write(to: config, options: .atomic)
    try FileManager.default.setAttributes(
        [.ownerAccountID: 0, .groupOwnerAccountID: 0, .posixPermissions: 0o600],
        ofItemAtPath: config.path
    )
    return StagedRuntime(dataDir: root, config: config.path)
}

private func validateStagedRuntime(_ runtime: StagedRuntime, mihomoBinary: String) throws {
    let validation = Process()
    validation.executableURL = URL(fileURLWithPath: mihomoBinary)
    validation.currentDirectoryURL = URL(fileURLWithPath: runtime.dataDir)
    validation.arguments = ["-t", "-d", runtime.dataDir, "-f", runtime.config]
    validation.environment = [:]
    validation.standardInput = FileHandle.nullDevice
    validation.standardOutput = FileHandle.nullDevice
    validation.standardError = FileHandle.nullDevice
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

private func createRootOwnedDirectory(_ path: String) throws {
    try FileManager.default.createDirectory(
        atPath: path,
        withIntermediateDirectories: true,
        attributes: [.posixPermissions: 0o700]
    )
    var current = URL(fileURLWithPath: "/Library/Application Support/Relay")
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
        throw HelperError.invalidExecutable("Relay.app code signature is unavailable")
    }
    let checkStatus = SecStaticCodeCheckValidity(
        code,
        SecCSFlags(rawValue: kSecCSStrictValidate),
        nil
    )
    guard checkStatus == errSecSuccess else {
        throw HelperError.invalidExecutable("Relay.app code signature does not seal Mihomo")
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
    appendLog("RELAY_REQUIRED_CLIENT_REQUIREMENT is not configured")
    Foundation.exit(1)
}
if !allowInsecureLocalRequirement
    && (!requiredClientRequirement.contains("anchor apple generic")
        || !requiredClientRequirement.contains("certificate leaf[subject.OU]")
        || !requiredClientRequirement.contains("identifier \"dev.relay.prototype.helperctl\""))
{
    appendLog("client code-signing requirement is not production-grade")
    Foundation.exit(1)
}
listener.setConnectionCodeSigningRequirement(requiredClientRequirement)
listener.delegate = delegate
listener.resume()
RunLoop.current.run()

import CryptoKit
import Darwin
import Foundation
import ServiceManagement
import Security

private let serviceName = "dev.manis.app.helper"
private let plistName = "dev.manis.app.helper.plist"
private let parentRequirementKey = "ManisParentCodeSigningRequirement"
private let insecureLocalKey = "ManisAllowInsecureLocalHelper"
private let localInstallerName = "manis-local-helper-install"
private let maximumCoreBytes: UInt64 = 128 * 1024 * 1024
private let maximumReleaseMetadataBytes = 1024 * 1024
private let maximumReleaseAssetBytes = 64 * 1024 * 1024
private let latestMihomoRelease = URL(
    string: "https://api.github.com/repos/MetaCubeX/mihomo/releases/latest"
)!
private let installedMihomo = URL(
    fileURLWithPath: "/Library/Application Support/Manis/bin/mihomo"
)

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

private enum CliError: Error, CustomStringConvertible {
    case usage
    case timeout
    case helper(String)
    case localInstaller(String)

    var description: String {
        switch self {
        case .usage:
            return """
                usage:
                  manis-helperctl register
                  manis-helperctl reinstall
                  manis-helperctl status
                  manis-helperctl stage-core
                  manis-helperctl start --data-dir PATH --config PATH --controller PATH
                  manis-helperctl stop
                """
        case .timeout:
            return "privileged helper did not reply"
        case .helper(let message):
            return message
        case .localInstaller(let message):
            return "local helper installation failed: \(message)"
        }
    }

    var exitStatus: Int32 {
        switch self {
        case .localInstaller:
            return 2
        case .usage, .timeout, .helper:
            return 1
        }
    }
}

private enum Command {
    case register
    case reinstall
    case status
    case stageCore
    case start(dataDir: String, config: String, controller: String)
    case stop
}

private func parseCommand(_ arguments: [String]) throws -> Command {
    guard let verb = arguments.first else {
        throw CliError.usage
    }
    switch verb {
    case "register":
        guard arguments.count == 1 else { throw CliError.usage }
        return .register
    case "reinstall":
        guard arguments.count == 1 else { throw CliError.usage }
        return .reinstall
    case "status":
        guard arguments.count == 1 else { throw CliError.usage }
        return .status
    case "stage-core":
        guard arguments.count == 1 else { throw CliError.usage }
        return .stageCore
    case "stop":
        guard arguments.count == 1 else { throw CliError.usage }
        return .stop
    case "start":
        guard arguments.count == 7 else { throw CliError.usage }
        var values: [String: String] = [:]
        var index = 1
        while index < arguments.count {
            values[arguments[index]] = arguments[index + 1]
            index += 2
        }
        guard let dataDir = values["--data-dir"],
            let config = values["--config"],
            let controller = values["--controller"]
        else {
            throw CliError.usage
        }
        return .start(dataDir: dataDir, config: config, controller: controller)
    default:
        throw CliError.usage
    }
}

private func registerService() throws {
    if insecureLocalBuild() {
        try installLocalService()
        return
    }
    let service = SMAppService.daemon(plistName: plistName)
    do {
        try service.register()
        print("registered")
    } catch {
        throw CliError.helper("register failed: \(error)")
    }
}

private func reinstallService() throws {
    if insecureLocalBuild() {
        try installLocalService()
        return
    }
    let service = SMAppService.daemon(plistName: plistName)
    if service.status != .notRegistered {
        let completion = DispatchSemaphore(value: 0)
        var unregisterError: Error?
        service.unregister { error in
            unregisterError = error
            completion.signal()
        }
        if completion.wait(timeout: .now() + 10) == .timedOut {
            throw CliError.helper("unregister outdated helper timed out")
        }
        if let unregisterError {
            throw CliError.helper("unregister outdated helper failed: \(unregisterError)")
        }
    }
    do {
        try service.register()
        print("registered")
    } catch {
        throw CliError.helper("register updated helper failed: \(error)")
    }
}

private func insecureLocalBuild() -> Bool {
    (Bundle.main.object(forInfoDictionaryKey: insecureLocalKey) as? Bool) == true
}

private func sha256(_ url: URL) throws -> String {
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    var hasher = SHA256()
    while let data = try handle.read(upToCount: 1024 * 1024), !data.isEmpty {
        hasher.update(data: data)
    }
    return hasher.finalize().map { String(format: "%02x", $0) }.joined()
}

private func managedCore() throws -> (Data, String) {
    let core = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library")
        .appendingPathComponent("Application Support")
        .appendingPathComponent("Manis")
        .appendingPathComponent("core")
        .appendingPathComponent("mihomo")
        .standardizedFileURL
    var metadata = stat()
    guard lstat(core.path, &metadata) == 0,
        metadata.st_mode & S_IFMT == S_IFREG,
        metadata.st_uid == getuid(),
        metadata.st_mode & 0o022 == 0,
        metadata.st_size > 0,
        UInt64(metadata.st_size) <= maximumCoreBytes,
        FileManager.default.isExecutableFile(atPath: core.path)
    else {
        throw CliError.helper("Manis-managed Mihomo is unavailable or unsafe")
    }
    let contents = try Data(contentsOf: core, options: [.mappedIfSafe])
    guard contents.count == metadata.st_size else {
        throw CliError.helper("Manis-managed Mihomo changed while it was read")
    }
    let digest = SHA256.hash(data: contents)
        .map { String(format: "%02x", $0) }
        .joined()
    guard try coreDigestIsTrusted(digest) else {
        throw CliError.helper(
            "Manis-managed Mihomo does not match the sealed seed, installed TUN core, or official latest release"
        )
    }
    return (contents, digest)
}

private func coreDigestIsTrusted(_ digest: String) throws -> Bool {
    let bundled = Bundle.main.bundleURL
        .appendingPathComponent("Contents")
        .appendingPathComponent("Resources")
        .appendingPathComponent("mihomo")
        .appendingPathComponent("mihomo")
        .standardizedFileURL
    if FileManager.default.isExecutableFile(atPath: bundled.path), try sha256(bundled) == digest {
        return true
    }
    var installedMetadata = stat()
    if lstat(installedMihomo.path, &installedMetadata) == 0,
        installedMetadata.st_mode & S_IFMT == S_IFREG,
        installedMetadata.st_uid == 0,
        installedMetadata.st_mode & 0o022 == 0,
        try sha256(installedMihomo) == digest
    {
        return true
    }
    return try latestStableReleaseDigest() == digest
}

private func latestStableReleaseDigest() throws -> String {
    let data = try downloadHTTPS(latestMihomoRelease, maximumBytes: maximumReleaseMetadataBytes)
    guard
        let release = try JSONSerialization.jsonObject(with: data) as? [String: Any],
        release["prerelease"] as? Bool == false,
        let tag = release["tag_name"] as? String,
        !tag.isEmpty,
        let assets = release["assets"] as? [[String: Any]]
    else {
        throw CliError.helper("could not verify the official latest Mihomo release")
    }
    let expectedNames: [String]
    #if arch(arm64)
        expectedNames = [
            "mihomo-darwin-arm64-go122-\(tag).gz",
            "mihomo-darwin-arm64-\(tag).gz",
        ]
    #elseif arch(x86_64)
        expectedNames = [
            "mihomo-darwin-amd64-v2-go122-\(tag).gz",
            "mihomo-darwin-amd64-v2-\(tag).gz",
        ]
    #else
        throw CliError.helper("unsupported macOS architecture for Mihomo")
    #endif
    for name in expectedNames {
        guard let asset = assets.first(where: { $0["name"] as? String == name }),
            let value = asset["digest"] as? String,
            value.hasPrefix("sha256:"),
            let downloadValue = asset["browser_download_url"] as? String,
            let downloadURL = URL(string: downloadValue),
            downloadURL.scheme == "https"
        else {
            continue
        }
        let packageDigest = String(value.dropFirst("sha256:".count)).lowercased()
        guard packageDigest.count == 64, packageDigest.allSatisfy(\.isHexDigit) else {
            continue
        }
        let archive = try downloadHTTPS(downloadURL, maximumBytes: maximumReleaseAssetBytes)
        let actualPackageDigest = SHA256.hash(data: archive)
            .map { String(format: "%02x", $0) }
            .joined()
        guard actualPackageDigest == packageDigest else {
            throw CliError.helper("official Mihomo release asset digest does not match")
        }
        return try unpackedGzipSha256(archive)
    }
    throw CliError.helper("official Mihomo release has no trusted digest for this Mac")
}

private func downloadHTTPS(_ url: URL, maximumBytes: Int) throws -> Data {
    guard url.scheme == "https" else {
        throw CliError.helper("trusted Mihomo release URL must use HTTPS")
    }
    let process = Process()
    let output = Pipe()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
    process.arguments = [
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto", "=https",
        "--proto-redir", "=https",
        "--max-redirs", "5",
        "--connect-timeout", "15",
        "--max-time", "120",
        "--max-filesize", String(maximumBytes),
        "--header", "Accept: application/vnd.github+json",
        "--user-agent", "Manis-mihomo-helper",
        url.absoluteString,
    ]
    process.environment = [:]
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    try process.run()
    var data = Data()
    var exceededLimit = false
    while let chunk = try output.fileHandleForReading.read(upToCount: 1024 * 1024), !chunk.isEmpty {
        if data.count > maximumBytes - chunk.count {
            exceededLimit = true
            process.terminate()
        } else if !exceededLimit {
            data.append(chunk)
        }
    }
    process.waitUntilExit()
    guard process.terminationStatus == 0, !data.isEmpty, !exceededLimit else {
        throw CliError.helper("could not download trusted Mihomo release data")
    }
    return data
}

private func unpackedGzipSha256(_ archive: Data) throws -> String {
    let process = Process()
    let input = Pipe()
    let output = Pipe()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/gzip")
    process.arguments = ["-dc"]
    process.environment = [:]
    process.standardInput = input
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    try process.run()
    DispatchQueue.global(qos: .utility).async {
        try? input.fileHandleForWriting.write(contentsOf: archive)
        try? input.fileHandleForWriting.close()
    }
    var hasher = SHA256()
    var count = 0
    var exceededLimit = false
    while let chunk = try output.fileHandleForReading.read(upToCount: 1024 * 1024), !chunk.isEmpty {
        count += chunk.count
        if count > maximumCoreBytes {
            exceededLimit = true
            process.terminate()
        } else if !exceededLimit {
            hasher.update(data: chunk)
        }
    }
    process.waitUntilExit()
    guard process.terminationStatus == 0, count > 0, !exceededLimit else {
        throw CliError.helper("official Mihomo release archive is invalid")
    }
    return hasher.finalize().map { String(format: "%02x", $0) }.joined()
}

private func installLocalService() throws {
    do {
        try reinstallLocalService()
    } catch let error as CliError {
        switch error {
        case .localInstaller:
            throw error
        case .usage, .timeout, .helper:
            throw CliError.localInstaller(error.description)
        }
    } catch {
        throw CliError.localInstaller(String(describing: error))
    }
}

private func reinstallLocalService() throws {
    let app = Bundle.main.bundleURL.resolvingSymlinksInPath().standardizedFileURL
    let installer = app
        .appendingPathComponent("Contents")
        .appendingPathComponent("MacOS")
        .appendingPathComponent(localInstallerName)
        .standardizedFileURL
    let helper = app
        .appendingPathComponent("Contents")
        .appendingPathComponent("Library")
        .appendingPathComponent("PrivilegedHelperTools")
        .appendingPathComponent(serviceName)
        .standardizedFileURL
    let mihomo = app
        .appendingPathComponent("Contents")
        .appendingPathComponent("Resources")
        .appendingPathComponent("mihomo")
        .appendingPathComponent("mihomo")
        .standardizedFileURL
    let expectedParent = app
        .appendingPathComponent("Contents")
        .appendingPathComponent("MacOS")
        .standardizedFileURL
    guard installer.deletingLastPathComponent() == expectedParent,
        FileManager.default.isExecutableFile(atPath: installer.path),
        FileManager.default.isExecutableFile(atPath: helper.path),
        FileManager.default.isExecutableFile(atPath: mihomo.path)
    else {
        throw CliError.localInstaller("local TUN helper payload is incomplete in Manis.app")
    }
    let installerHash = try sha256(installer)
    let helperHash = try sha256(helper)
    let mihomoHash = try sha256(mihomo)
    let allowedUser = String(getuid())

    let script = #"""
        on run argv
            set installerPath to item 1 of argv
            set appPath to item 2 of argv
            set expectedInstallerHash to item 3 of argv
            set expectedHelperHash to item 4 of argv
            set expectedMihomoHash to item 5 of argv
            set allowedUser to item 6 of argv
            set commandText to "set -e; temporary=$(/usr/bin/mktemp /var/tmp/manis-local-helper-install.XXXXXX); trap '/bin/rm -f \"$temporary\"' EXIT; /bin/cp " & quoted form of installerPath & " \"$temporary\"; actual=$(/usr/bin/shasum -a 256 \"$temporary\" | /usr/bin/cut -d ' ' -f 1); /bin/test \"$actual\" = " & quoted form of expectedInstallerHash & "; /bin/chmod 0700 \"$temporary\"; \"$temporary\" reinstall " & quoted form of appPath & " " & quoted form of expectedHelperHash & " " & quoted form of expectedMihomoHash & " " & quoted form of allowedUser
            do shell script commandText with administrator privileges with prompt "Manis needs administrator access to install its local TUN helper."
        end run
        """#
    let process = Process()
    let output = Pipe()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    process.arguments = [
        "-e", script, installer.path, app.path, installerHash, helperHash, mihomoHash, allowedUser,
    ]
    process.standardOutput = output
    process.standardError = output
    process.standardInput = FileHandle.nullDevice
    try process.run()
    process.waitUntilExit()
    let message = String(
        data: output.fileHandleForReading.readDataToEndOfFile(),
        encoding: .utf8
    )?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    guard process.terminationStatus == 0 else {
        throw CliError.localInstaller(
            message.isEmpty
                ? "installer exited with status \(process.terminationStatus)"
                : message
        )
    }
    print(message.isEmpty ? "registered local development helper" : message)
}

private func validateParentProcess() throws {
    guard let requirement = Bundle.main.object(forInfoDictionaryKey: parentRequirementKey) as? String,
        !requirement.isEmpty
    else {
        throw CliError.helper("Manis parent code-signing requirement is missing")
    }
    let allowInsecure = insecureLocalBuild()
    if !allowInsecure
        && (!requirement.contains("anchor apple generic")
            || !requirement.contains("certificate leaf[subject.OU]")
            || !requirement.contains("identifier \"dev.manis.app\""))
    {
        throw CliError.helper("Manis parent code-signing requirement is not production-grade")
    }

    var parentCode: SecCode?
    let attributes = [kSecGuestAttributePid: NSNumber(value: getppid())] as CFDictionary
    var status = SecCodeCopyGuestWithAttributes(nil, attributes, [], &parentCode)
    guard status == errSecSuccess, let parentCode else {
        throw CliError.helper("could not inspect Manis parent process")
    }
    var parentRequirement: SecRequirement?
    status = SecRequirementCreateWithString(requirement as CFString, [], &parentRequirement)
    guard status == errSecSuccess, let parentRequirement else {
        throw CliError.helper("Manis parent code-signing requirement is invalid")
    }
    status = SecCodeCheckValidity(parentCode, [], parentRequirement)
    guard status == errSecSuccess else {
        throw CliError.helper("manis-helperctl must be launched directly by Manis.app")
    }
}

private func callHelper(
    timeout: DispatchTimeInterval = .seconds(10),
    _ invoke: @escaping (ManisPrivilegedHelperProtocol, @escaping (String, Int32) -> Void) -> Void
) throws {
    let connection = NSXPCConnection(machServiceName: serviceName, options: .privileged)
    connection.remoteObjectInterface = NSXPCInterface(with: ManisPrivilegedHelperProtocol.self)
    connection.resume()
    defer { connection.invalidate() }

    var result: (String, Int32)?
    var proxyError: Error?
    let semaphore = DispatchSemaphore(value: 0)
    let proxy = connection.remoteObjectProxyWithErrorHandler { error in
        proxyError = error
        semaphore.signal()
    } as? ManisPrivilegedHelperProtocol
    guard let proxy else {
        throw CliError.helper("could not create helper proxy")
    }
    invoke(proxy) { message, code in
        result = (message, code)
        semaphore.signal()
    }
    if semaphore.wait(timeout: .now() + timeout) == .timedOut {
        throw CliError.timeout
    }
    if let error = proxyError {
        throw CliError.helper("helper connection failed: \(error)")
    }
    guard let (message, code) = result else {
        throw CliError.helper("helper returned no response")
    }
    if code == 0 {
        print(message)
    } else {
        fputs("\(message)\n", stderr)
        Foundation.exit(code)
    }
}

do {
    try validateParentProcess()
    let command = try parseCommand(Array(CommandLine.arguments.dropFirst()))
    switch command {
    case .register:
        try registerService()
    case .reinstall:
        try reinstallService()
    case .status:
        try callHelper(timeout: .seconds(2)) { helper, reply in helper.status(withReply: reply) }
    case .stageCore:
        let (contents, digest) = try managedCore()
        try callHelper(timeout: .seconds(30)) { helper, reply in
            helper.stageCore(contents: contents, sha256: digest, withReply: reply)
        }
    case .stop:
        try callHelper { helper, reply in helper.stop(withReply: reply) }
    case .start(let dataDir, let config, let controller):
        try callHelper { helper, reply in
            helper.start(dataDir: dataDir, config: config, controller: controller, withReply: reply)
        }
    }
} catch {
    fputs("\(error)\n", stderr)
    Foundation.exit((error as? CliError)?.exitStatus ?? 1)
}

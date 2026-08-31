import CryptoKit
import Darwin
import Foundation
import ServiceManagement
import Security

private let serviceName = "dev.manis.app.helper"
private let plistName = "dev.manis.app.helper.plist"
private let localInstallerName = "manis-local-helper-install"
private let maximumCoreBytes: UInt64 = 128 * 1024 * 1024
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
    func stop(expectedPid: pid_t, withReply reply: @escaping (String, Int32) -> Void)
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
                  manis-helperctl stop --pid PID
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
    case stop(expectedPid: pid_t)
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
        guard arguments.count == 3,
            arguments[1] == "--pid",
            let expectedPid = parsePositivePid(arguments[2])
        else { throw CliError.usage }
        return .stop(expectedPid: expectedPid)
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

private func parsePositivePid(_ value: String) -> pid_t? {
    guard let pid = pid_t(value), pid > 0 else {
        return nil
    }
    return pid
}

private func registerService() throws {
    if administratorInstalledBuild {
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
    if administratorInstalledBuild {
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

// Compile this choice into the controller: an attacker must not be able to switch an approved
// controller into another authentication mode by copying it into a different app's Info.plist.
#if MANIS_ADMINISTRATOR_HELPER
private let administratorInstalledBuild = true
#else
private let administratorInstalledBuild = false
#endif

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
    // The administrator-installed daemon independently verifies provenance. Signed builds keep
    // the controller's existing provenance check as well as the helper's sealed-bundle check.
    guard try administratorInstalledBuild || signedBuildCoreDigestIsTrusted(digest) else {
        throw CliError.helper(
            "Manis-managed Mihomo does not match the sealed seed, installed TUN core, or official latest release"
        )
    }
    return (contents, digest)
}

private func signedBuildCoreDigestIsTrusted(_ digest: String) throws -> Bool {
    let bundled = Bundle.main.bundleURL
        .appendingPathComponent("Contents/Resources/mihomo/mihomo")
        .standardizedFileURL
    if FileManager.default.isExecutableFile(atPath: bundled.path), try sha256(bundled) == digest {
        return true
    }
    var installedMetadata = stat()
    if lstat(installedMihomo.path, &installedMetadata) == 0,
        installedMetadata.st_mode & S_IFMT == S_IFREG,
        installedMetadata.st_uid == 0,
        installedMetadata.st_mode & 0o022 == 0
    {
        try ManisHelperSecurity.requireRootOwnedPath(installedMihomo.path, directory: false)
        if try sha256(installedMihomo) == digest { return true }
    }
    return try MihomoReleaseVerifier.latestDigest() == digest
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
    let parentRequirement = try ManisHelperSecurity.pinnedRequirement(at: app, identifier: "dev.manis.app")
    let allowedUser = String(getuid())

    let script = #"""
        on run argv
            set installerPath to item 1 of argv
            set appPath to item 2 of argv
            set expectedInstallerHash to item 3 of argv
            set expectedHelperHash to item 4 of argv
            set expectedMihomoHash to item 5 of argv
            set allowedUser to item 6 of argv
            set expectedParentRequirement to item 7 of argv
            set commandText to "set -e; temporary=$(/usr/bin/mktemp /var/tmp/manis-local-helper-install.XXXXXX); trap '/bin/rm -f \"$temporary\"' EXIT; /bin/cp " & quoted form of installerPath & " \"$temporary\"; actual=$(/usr/bin/shasum -a 256 \"$temporary\" | /usr/bin/cut -d ' ' -f 1); /bin/test \"$actual\" = " & quoted form of expectedInstallerHash & "; /bin/chmod 0700 \"$temporary\"; \"$temporary\" reinstall " & quoted form of appPath & " " & quoted form of expectedParentRequirement & " " & quoted form of expectedHelperHash & " " & quoted form of expectedMihomoHash & " " & quoted form of allowedUser
            do shell script commandText with administrator privileges with prompt ("Manis needs to install or update its TUN helper for: " & appPath)
        end run
        """#
    let process = Process()
    let output = Pipe()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
    process.arguments = [
        "-e", script, installer.path, app.path, installerHash, helperHash, mihomoHash, allowedUser,
        parentRequirement,
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
    print(message.isEmpty ? "registered administrator-approved helper" : message)
}

private func validateParentProcess(for command: Command) throws {
    let app = Bundle.main.bundleURL.resolvingSymlinksInPath().standardizedFileURL
    let requirement: String
    if administratorInstalledBuild {
        switch command {
        case .register, .reinstall:
            // A new version may request approval, but cannot call the daemon until the user
            // approves its exact app and controller fingerprints in the root-owned policy.
            requirement = try ManisHelperSecurity.pinnedRequirement(at: app, identifier: "dev.manis.app")
        case .status, .stageCore, .start, .stop:
            let approval = try ManisHelperSecurity.installedApproval()
            let controller = app.appendingPathComponent("Contents/MacOS/manis-helperctl")
            guard approval.user == getuid(),
                try ManisHelperSecurity.pinnedRequirement(at: controller, identifier: "dev.manis.app.helperctl")
                    == approval.client
            else { throw CliError.helper("this Manis version requires administrator approval for TUN") }
            requirement = approval.parent
        }
    } else {
        // Derive the Team ID from our own signature, never from a caller-editable Info.plist.
        requirement = try ManisHelperSecurity.ownTeamRequirement(identifier: "dev.manis.app")
    }
    try ManisHelperSecurity.validateParent(bundle: app, requirement: requirement)
}

private func callHelper(
    timeout: DispatchTimeInterval = .seconds(10),
    _ invoke: @escaping (ManisPrivilegedHelperProtocol, @escaping (String, Int32) -> Void) -> Void
) throws {
    let connection = NSXPCConnection(machServiceName: serviceName, options: .privileged)
    let helperRequirement = try administratorInstalledBuild
        ? ManisHelperSecurity.installedApproval().helper
        : ManisHelperSecurity.ownTeamRequirement(identifier: "dev.manis.app.helper")
    connection.setCodeSigningRequirement(helperRequirement)
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
    let command = try parseCommand(Array(CommandLine.arguments.dropFirst()))
    try validateParentProcess(for: command)
    switch command {
    case .register:
        try registerService()
    case .reinstall:
        try reinstallService()
    case .status:
        try callHelper(timeout: .seconds(2)) { helper, reply in helper.status(withReply: reply) }
    case .stageCore:
        let (contents, digest) = try managedCore()
        try callHelper(timeout: .seconds(300)) { helper, reply in
            helper.stageCore(contents: contents, sha256: digest, withReply: reply)
        }
    case .stop(let expectedPid):
        try callHelper { helper, reply in helper.stop(expectedPid: expectedPid, withReply: reply) }
    case .start(let dataDir, let config, let controller):
        try callHelper { helper, reply in
            helper.start(dataDir: dataDir, config: config, controller: controller, withReply: reply)
        }
    }
} catch {
    fputs("\(error)\n", stderr)
    Foundation.exit((error as? CliError)?.exitStatus ?? 1)
}

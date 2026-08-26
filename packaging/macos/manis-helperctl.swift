import CryptoKit
import Darwin
import Foundation
import ServiceManagement
import Security
import Darwin

private let serviceName = "dev.manis.app.helper"
private let plistName = "dev.manis.app.helper.plist"
private let parentRequirementKey = "ManisParentCodeSigningRequirement"
private let insecureLocalKey = "ManisAllowInsecureLocalHelper"
private let localInstallerName = "manis-local-helper-install"

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
}

private enum CliError: Error, CustomStringConvertible {
    case usage
    case timeout
    case helper(String)

    var description: String {
        switch self {
        case .usage:
            return """
                usage:
                  manis-helperctl register
                  manis-helperctl reinstall
                  manis-helperctl status
                  manis-helperctl start --data-dir PATH --config PATH --controller PATH
                  manis-helperctl stop
                """
        case .timeout:
            return "privileged helper did not reply"
        case .helper(let message):
            return message
        }
    }
}

private enum Command {
    case register
    case reinstall
    case status
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
        try reinstallLocalService()
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
        try reinstallLocalService()
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
        throw CliError.helper("local TUN helper payload is incomplete in Manis.app")
    }
    let installerHash = try sha256(installer)
    let helperHash = try sha256(helper)
    let mihomoHash = try sha256(mihomo)
    let allowedUser = String(getuid())

    let script = """
        on run argv
            set installerPath to item 1 of argv
            set appPath to item 2 of argv
            set expectedInstallerHash to item 3 of argv
            set expectedHelperHash to item 4 of argv
            set expectedMihomoHash to item 5 of argv
            set allowedUser to item 6 of argv
            set commandText to "set -e; temporary=$(/usr/bin/mktemp /var/tmp/manis-local-helper-install.XXXXXX); trap '/bin/rm -f \"$temporary\"' EXIT; /bin/cp " & quoted form of installerPath & " \"$temporary\"; actual=$(/usr/bin/shasum -a 256 \"$temporary\" | /usr/bin/cut -d ' ' -f 1); /usr/bin/test \"$actual\" = " & quoted form of expectedInstallerHash & "; /bin/chmod 0700 \"$temporary\"; \"$temporary\" reinstall " & quoted form of appPath & " " & quoted form of expectedHelperHash & " " & quoted form of expectedMihomoHash & " " & quoted form of allowedUser
            do shell script commandText with administrator privileges with prompt "Manis needs administrator access to install its local TUN helper."
        end run
        """
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
        throw CliError.helper(
            message.isEmpty
                ? "local helper installation failed with status \(process.terminationStatus)"
                : "local helper installation failed: \(message)"
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
    case .stop:
        try callHelper { helper, reply in helper.stop(withReply: reply) }
    case .start(let dataDir, let config, let controller):
        try callHelper { helper, reply in
            helper.start(dataDir: dataDir, config: config, controller: controller, withReply: reply)
        }
    }
} catch {
    fputs("\(error)\n", stderr)
    Foundation.exit(1)
}

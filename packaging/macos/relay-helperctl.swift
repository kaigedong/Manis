import Foundation
import ServiceManagement
import Security
import Darwin

private let serviceName = "dev.relay.prototype.helper"
private let plistName = "dev.relay.prototype.helper.plist"
private let parentRequirementKey = "RelayParentCodeSigningRequirement"
private let insecureLocalKey = "RelayAllowInsecureLocalHelper"

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

private enum CliError: Error, CustomStringConvertible {
    case usage
    case timeout
    case helper(String)

    var description: String {
        switch self {
        case .usage:
            return """
                usage:
                  relay-helperctl register
                  relay-helperctl reinstall
                  relay-helperctl status
                  relay-helperctl start --data-dir PATH --config PATH --controller PATH
                  relay-helperctl stop
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
    let service = SMAppService.daemon(plistName: plistName)
    do {
        try service.register()
        print("registered")
    } catch {
        throw CliError.helper("register failed: \(error)")
    }
}

private func reinstallService() throws {
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

private func validateParentProcess() throws {
    guard let requirement = Bundle.main.object(forInfoDictionaryKey: parentRequirementKey) as? String,
        !requirement.isEmpty
    else {
        throw CliError.helper("Relay parent code-signing requirement is missing")
    }
    let allowInsecure =
        (Bundle.main.object(forInfoDictionaryKey: insecureLocalKey) as? Bool) == true
    if !allowInsecure
        && (!requirement.contains("anchor apple generic")
            || !requirement.contains("certificate leaf[subject.OU]")
            || !requirement.contains("identifier \"dev.relay.prototype\""))
    {
        throw CliError.helper("Relay parent code-signing requirement is not production-grade")
    }

    var parentCode: SecCode?
    let attributes = [kSecGuestAttributePid: NSNumber(value: getppid())] as CFDictionary
    var status = SecCodeCopyGuestWithAttributes(nil, attributes, [], &parentCode)
    guard status == errSecSuccess, let parentCode else {
        throw CliError.helper("could not inspect Relay parent process")
    }
    var parentRequirement: SecRequirement?
    status = SecRequirementCreateWithString(requirement as CFString, [], &parentRequirement)
    guard status == errSecSuccess, let parentRequirement else {
        throw CliError.helper("Relay parent code-signing requirement is invalid")
    }
    status = SecCodeCheckValidity(parentCode, [], parentRequirement)
    guard status == errSecSuccess else {
        throw CliError.helper("relay-helperctl must be launched directly by Relay.app")
    }
}

private func callHelper(_ invoke: @escaping (RelayPrivilegedHelperProtocol, @escaping (String, Int32) -> Void) -> Void) throws {
    let connection = NSXPCConnection(machServiceName: serviceName, options: .privileged)
    connection.remoteObjectInterface = NSXPCInterface(with: RelayPrivilegedHelperProtocol.self)
    connection.resume()
    defer { connection.invalidate() }

    var result: (String, Int32)?
    var proxyError: Error?
    let semaphore = DispatchSemaphore(value: 0)
    let proxy = connection.remoteObjectProxyWithErrorHandler { error in
        proxyError = error
        semaphore.signal()
    } as? RelayPrivilegedHelperProtocol
    guard let proxy else {
        throw CliError.helper("could not create helper proxy")
    }
    invoke(proxy) { message, code in
        result = (message, code)
        semaphore.signal()
    }
    if semaphore.wait(timeout: .now() + 10) == .timedOut {
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
        try callHelper { helper, reply in helper.status(withReply: reply) }
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

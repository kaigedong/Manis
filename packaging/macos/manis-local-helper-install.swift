import CryptoKit
import Darwin
import Foundation

private let localServiceLabel = "dev.manis.app.helper.local"
private let machServiceName = "dev.manis.app.helper"
private let helperControlRequirement = "identifier \"dev.manis.app.helperctl\""
private let installedHelper = "/Library/PrivilegedHelperTools/dev.manis.app.helper.local"
private let installedMihomo = "/Library/Application Support/Manis/bin/mihomo"
private let installedPlist = "/Library/LaunchDaemons/dev.manis.app.helper.local.plist"

private enum InstallerError: Error, CustomStringConvertible {
    case invalidInvocation
    case invalidSource(String)
    case unsafeDestination(String)
    case commandFailed(String)

    var description: String {
        switch self {
        case .invalidInvocation:
            return "usage: manis-local-helper-install reinstall"
        case .invalidSource(let message), .unsafeDestination(let message),
            .commandFailed(let message):
            return message
        }
    }
}

private func run(
    _ executable: String,
    _ arguments: [String],
    allowFailure: Bool = false
) throws -> (Int32, String) {
    let process = Process()
    let output = Pipe()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.standardOutput = output
    process.standardError = output
    process.standardInput = FileHandle.nullDevice
    try process.run()
    process.waitUntilExit()
    let message = String(
        data: output.fileHandleForReading.readDataToEndOfFile(),
        encoding: .utf8
    )?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if !allowFailure && process.terminationStatus != 0 {
        throw InstallerError.commandFailed(
            message.isEmpty
                ? "\(executable) failed with status \(process.terminationStatus)"
                : message
        )
    }
    return (process.terminationStatus, message)
}

private func applicationBundleURL(_ path: String) throws -> URL {
    let app = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
    guard app.pathExtension == "app",
        app.lastPathComponent == "Manis.app"
    else {
        throw InstallerError.invalidSource("local helper source must be Manis.app")
    }
    return app
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

private func validSha256(_ value: String) -> Bool {
    value.count == 64 && value.allSatisfy { $0.isHexDigit && !$0.isUppercase }
}

private func requireRegularSource(_ url: URL) throws {
    var metadata = stat()
    guard lstat(url.path, &metadata) == 0,
        metadata.st_mode & S_IFMT == S_IFREG,
        metadata.st_mode & S_IWOTH == 0
    else {
        throw InstallerError.invalidSource("unsafe packaged helper source: \(url.path)")
    }
}

private func ensureRootOwnedDirectory(_ path: String, mode: mode_t) throws {
    let manager = FileManager.default
    if !manager.fileExists(atPath: path) {
        try manager.createDirectory(
            atPath: path,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: NSNumber(value: mode)]
        )
    }
    var metadata = stat()
    guard lstat(path, &metadata) == 0,
        metadata.st_mode & S_IFMT == S_IFDIR,
        metadata.st_uid == 0,
        metadata.st_mode & 0o022 == 0
    else {
        throw InstallerError.unsafeDestination("unsafe root service directory: \(path)")
    }
    guard chown(path, 0, 0) == 0, chmod(path, mode) == 0 else {
        throw InstallerError.unsafeDestination("could not secure root service directory: \(path)")
    }
}

private func rejectSymlink(_ path: String) throws {
    var metadata = stat()
    if lstat(path, &metadata) == 0, metadata.st_mode & S_IFMT == S_IFLNK {
        throw InstallerError.unsafeDestination("refusing symlink destination: \(path)")
    }
}

private func installFile(
    source: URL,
    destination: String,
    mode: mode_t,
    expectedHash: String
) throws {
    try rejectSymlink(destination)
    let destinationURL = URL(fileURLWithPath: destination)
    let temporary = destinationURL.deletingLastPathComponent().appendingPathComponent(
        ".\(destinationURL.lastPathComponent).\(UUID().uuidString).tmp"
    )
    defer { try? FileManager.default.removeItem(at: temporary) }
    try FileManager.default.copyItem(at: source, to: temporary)
    guard try sha256(temporary) == expectedHash else {
        throw InstallerError.invalidSource("packaged payload changed during authorization")
    }
    guard chown(temporary.path, 0, 0) == 0, chmod(temporary.path, mode) == 0 else {
        throw InstallerError.unsafeDestination("could not secure staged file: \(destination)")
    }
    guard rename(temporary.path, destination) == 0 else {
        throw InstallerError.unsafeDestination("could not install fixed helper file: \(destination)")
    }
}

private func localLaunchDaemonPlist(allowedUser: uid_t) throws -> Data {
    let plist: [String: Any] = [
        "Label": localServiceLabel,
        "ProgramArguments": [installedHelper],
        "MachServices": [machServiceName: true],
        "RunAtLoad": true,
        "EnvironmentVariables": [
            "MANIS_REQUIRED_CLIENT_REQUIREMENT": helperControlRequirement,
            "MANIS_ALLOW_INSECURE_LOCAL_HELPER": "1",
            "MANIS_INSECURE_LOCAL_MIHOMO": installedMihomo,
            "MANIS_LOCAL_ALLOWED_UID": String(allowedUser),
        ],
    ]
    return try PropertyListSerialization.data(
        fromPropertyList: plist,
        format: .xml,
        options: 0
    )
}

private func installPlist(allowedUser: uid_t) throws {
    try rejectSymlink(installedPlist)
    let destination = URL(fileURLWithPath: installedPlist)
    let temporary = destination.deletingLastPathComponent().appendingPathComponent(
        ".\(destination.lastPathComponent).\(UUID().uuidString).tmp"
    )
    defer { try? FileManager.default.removeItem(at: temporary) }
    try localLaunchDaemonPlist(allowedUser: allowedUser).write(to: temporary, options: .atomic)
    guard chown(temporary.path, 0, 0) == 0, chmod(temporary.path, 0o644) == 0 else {
        throw InstallerError.unsafeDestination("could not secure staged launch daemon plist")
    }
    guard rename(temporary.path, installedPlist) == 0 else {
        throw InstallerError.unsafeDestination("could not install launch daemon plist")
    }
}

private func reinstall(
    appPath: String,
    expectedHelperHash: String,
    expectedMihomoHash: String,
    allowedUser: uid_t
) throws {
    guard geteuid() == 0 else {
        throw InstallerError.invalidSource("administrator authorization is required")
    }
    guard allowedUser > 0,
        validSha256(expectedHelperHash),
        validSha256(expectedMihomoHash)
    else {
        throw InstallerError.invalidInvocation
    }
    let app = try applicationBundleURL(appPath)
    let helper = app.appendingPathComponent(
        "Contents/Library/PrivilegedHelperTools/dev.manis.app.helper"
    )
    let mihomo = app.appendingPathComponent("Contents/Resources/mihomo/mihomo")
    try requireRegularSource(helper)
    try requireRegularSource(mihomo)
    _ = try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", app.path])

    try ensureRootOwnedDirectory("/Library/PrivilegedHelperTools", mode: 0o755)
    try ensureRootOwnedDirectory("/Library/LaunchDaemons", mode: 0o755)
    try ensureRootOwnedDirectory("/Library/Application Support/Manis", mode: 0o755)
    try ensureRootOwnedDirectory("/Library/Application Support/Manis/bin", mode: 0o755)

    _ = try run(
        "/bin/launchctl",
        ["bootout", "system/\(localServiceLabel)"],
        allowFailure: true
    )
    try installFile(
        source: helper,
        destination: installedHelper,
        mode: 0o755,
        expectedHash: expectedHelperHash
    )
    try installFile(
        source: mihomo,
        destination: installedMihomo,
        mode: 0o755,
        expectedHash: expectedMihomoHash
    )
    try installPlist(allowedUser: allowedUser)
    _ = try run("/bin/launchctl", ["bootstrap", "system", installedPlist])
    _ = try run("/bin/launchctl", ["enable", "system/\(localServiceLabel)"])
    _ = try run("/bin/launchctl", ["kickstart", "-k", "system/\(localServiceLabel)"])
    print("registered local development helper")
}

do {
    guard CommandLine.arguments.count == 6,
        CommandLine.arguments[1] == "reinstall",
        let allowedUser = uid_t(CommandLine.arguments[5])
    else {
        throw InstallerError.invalidInvocation
    }
    try reinstall(
        appPath: CommandLine.arguments[2],
        expectedHelperHash: CommandLine.arguments[3],
        expectedMihomoHash: CommandLine.arguments[4],
        allowedUser: allowedUser
    )
} catch {
    fputs("\(error)\n", stderr)
    Foundation.exit(1)
}

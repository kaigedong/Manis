import CryptoKit
import Darwin
import Foundation

private let localServiceLabel = "dev.manis.app.helper.local"
private let machServiceName = "dev.manis.app.helper"
private let installedHelper = "/Library/PrivilegedHelperTools/dev.manis.app.helper.local"
private let installedMihomo = "/Library/Application Support/Manis/bin/mihomo"
private let installedPlist = "/Library/LaunchDaemons/dev.manis.app.helper.local.plist"
private let parentIdentifier = "dev.manis.app"
private let helperControlIdentifier = "dev.manis.app.helperctl"
private let helperIdentifier = "dev.manis.app.helper"
private let helperControlRelativePath = "Contents/MacOS/manis-helperctl"
private let helperRelativePath = "Contents/Library/PrivilegedHelperTools/dev.manis.app.helper"
private let mihomoRelativePath = "Contents/Resources/mihomo/mihomo"

private enum InstallerError: Error, CustomStringConvertible {
    case invalidInvocation
    case invalidSource(String)
    case unsafeDestination(String)
    case commandFailed(String)

    var description: String {
        switch self {
        case .invalidInvocation:
            return
                "usage: manis-local-helper-install reinstall APP_PATH EXPECTED_APP_REQUIREMENT EXPECTED_HELPER_SHA256 EXPECTED_MIHOMO_SHA256 ALLOWED_UID"
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
    process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
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

private func makeSecureStagingDirectory() throws -> URL {
    var template = Array("/var/tmp/manis-local-helper-install.XXXXXX".utf8CString)
    guard let directory = template.withUnsafeMutableBufferPointer({ buffer in
        mkdtemp(buffer.baseAddress)
    }) else {
        throw InstallerError.unsafeDestination("could not create secure installer staging directory")
    }
    let staging = URL(fileURLWithPath: String(cString: directory), isDirectory: true)
    guard chown(staging.path, 0, 0) == 0, chmod(staging.path, 0o700) == 0 else {
        throw InstallerError.unsafeDestination("could not secure installer staging directory")
    }
    var metadata = stat()
    guard lstat(staging.path, &metadata) == 0,
        metadata.st_mode & S_IFMT == S_IFDIR,
        metadata.st_uid == 0,
        metadata.st_mode & 0o077 == 0
    else {
        throw InstallerError.unsafeDestination("unsafe installer staging directory")
    }
    return staging
}

private func snapshotApplicationBundle(_ app: URL, in staging: URL) throws -> URL {
    let stagedApp = staging.appendingPathComponent("Manis.app", isDirectory: true)
    _ = try run("/usr/bin/ditto", [app.path, stagedApp.path])
    var metadata = stat()
    guard lstat(stagedApp.path, &metadata) == 0,
        metadata.st_mode & S_IFMT == S_IFDIR,
        metadata.st_mode & S_IWOTH == 0
    else {
        throw InstallerError.invalidSource("could not snapshot Manis.app for authorization")
    }
    return stagedApp
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

private func requireBundlePayload(_ app: URL, relativePath: String) throws -> URL {
    let components = relativePath.split(separator: "/")
    guard !components.isEmpty else {
        throw InstallerError.invalidSource("empty packaged payload path")
    }
    var current = app
    for (index, component) in components.enumerated() {
        guard component != "." && component != ".." else {
            throw InstallerError.invalidSource("unsafe packaged payload path: \(relativePath)")
        }
        current.appendPathComponent(String(component), isDirectory: index != components.count - 1)
        var metadata = stat()
        let expectedType = index == components.count - 1 ? S_IFREG : S_IFDIR
        guard lstat(current.path, &metadata) == 0,
            metadata.st_mode & S_IFMT == expectedType,
            metadata.st_mode & S_IWOTH == 0
        else {
            throw InstallerError.invalidSource("unsafe packaged payload path: \(relativePath)")
        }
    }
    return current.standardizedFileURL
}

private func pathMetadata(_ path: String) throws -> stat? {
    var metadata = stat()
    if lstat(path, &metadata) == 0 {
        return metadata
    }
    guard errno == ENOENT else {
        throw InstallerError.unsafeDestination("could not inspect root service path: \(path)")
    }
    return nil
}

private func requireSafeRootDirectory(_ path: String) throws {
    guard let metadata = try pathMetadata(path),
        metadata.st_mode & S_IFMT == S_IFDIR,
        metadata.st_uid == 0,
        metadata.st_mode & 0o022 == 0
    else {
        throw InstallerError.unsafeDestination("unsafe root service directory: \(path)")
    }
}

private func ensureRootOwnedDirectory(_ path: String, mode: mode_t) throws {
    guard path.hasPrefix("/"),
        URL(fileURLWithPath: path).standardizedFileURL.path == path
    else {
        throw InstallerError.unsafeDestination("invalid root service directory: \(path)")
    }
    let manager = FileManager.default
    var current = URL(fileURLWithPath: "/", isDirectory: true)
    let components = URL(fileURLWithPath: path, isDirectory: true).pathComponents.dropFirst()
    for component in components {
        current.appendPathComponent(component, isDirectory: true)
        if try pathMetadata(current.path) == nil {
            try manager.createDirectory(
                at: current,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: NSNumber(value: mode)]
            )
            guard chown(current.path, 0, 0) == 0, chmod(current.path, mode) == 0 else {
                throw InstallerError.unsafeDestination(
                    "could not secure root service directory: \(current.path)"
                )
            }
        }
        try requireSafeRootDirectory(current.path)
    }
    guard chown(path, 0, 0) == 0, chmod(path, mode) == 0 else {
        throw InstallerError.unsafeDestination("could not secure root service directory: \(path)")
    }
    try ManisHelperSecurity.requireRootOwnedPath(path, directory: true)
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
    try ManisHelperSecurity.requireRootOwnedPath(
        destinationURL.deletingLastPathComponent().path,
        directory: true
    )
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
    try ManisHelperSecurity.requireRootOwnedPath(destination, directory: false)
}

private func localLaunchDaemonPlist(
    allowedUser: uid_t,
    clientRequirement: String,
    parentRequirement: String,
    helperRequirement: String,
    approvedSeedHash: String
) throws -> Data {
    let plist: [String: Any] = [
        "Label": localServiceLabel,
        "ProgramArguments": [installedHelper],
        "MachServices": [machServiceName: true],
        "RunAtLoad": true,
        "EnvironmentVariables": [
            "MANIS_ADMINISTRATOR_INSTALL": "1",
            "MANIS_REQUIRED_CLIENT_REQUIREMENT": clientRequirement,
            "MANIS_REQUIRED_PARENT_REQUIREMENT": parentRequirement,
            "MANIS_REQUIRED_HELPER_REQUIREMENT": helperRequirement,
            "MANIS_APPROVED_SEED_SHA256": approvedSeedHash,
            "MANIS_LOCAL_ALLOWED_UID": String(allowedUser),
        ],
    ]
    return try PropertyListSerialization.data(
        fromPropertyList: plist,
        format: .xml,
        options: 0
    )
}

private func installPlist(
    allowedUser: uid_t,
    clientRequirement: String,
    parentRequirement: String,
    helperRequirement: String,
    approvedSeedHash: String
) throws {
    try rejectSymlink(installedPlist)
    let destination = URL(fileURLWithPath: installedPlist)
    try ManisHelperSecurity.requireRootOwnedPath(
        destination.deletingLastPathComponent().path,
        directory: true
    )
    let temporary = destination.deletingLastPathComponent().appendingPathComponent(
        ".\(destination.lastPathComponent).\(UUID().uuidString).tmp"
    )
    defer { try? FileManager.default.removeItem(at: temporary) }
    try localLaunchDaemonPlist(
        allowedUser: allowedUser,
        clientRequirement: clientRequirement,
        parentRequirement: parentRequirement,
        helperRequirement: helperRequirement,
        approvedSeedHash: approvedSeedHash
    ).write(to: temporary, options: .atomic)
    guard chown(temporary.path, 0, 0) == 0, chmod(temporary.path, 0o644) == 0 else {
        throw InstallerError.unsafeDestination("could not secure staged launch daemon plist")
    }
    guard rename(temporary.path, installedPlist) == 0 else {
        throw InstallerError.unsafeDestination("could not install launch daemon plist")
    }
    try ManisHelperSecurity.requireRootOwnedPath(installedPlist, directory: false)
}

private func reinstall(
    appPath: String,
    expectedAppRequirement: String,
    expectedHelperHash: String,
    expectedMihomoHash: String,
    allowedUser: uid_t
) throws {
    guard geteuid() == 0 else {
        throw InstallerError.invalidSource("administrator authorization is required")
    }
    guard allowedUser > 0,
        ManisHelperSecurity.validatePinnedRequirement(
            expectedAppRequirement,
            identifier: parentIdentifier
        ),
        validSha256(expectedHelperHash),
        validSha256(expectedMihomoHash)
    else {
        throw InstallerError.invalidInvocation
    }
    let app = try applicationBundleURL(appPath)
    let staging = try makeSecureStagingDirectory()
    defer { try? FileManager.default.removeItem(at: staging) }
    let stagedApp = try snapshotApplicationBundle(app, in: staging)
    let helperControl = try requireBundlePayload(
        stagedApp,
        relativePath: helperControlRelativePath
    )
    let helper = try requireBundlePayload(stagedApp, relativePath: helperRelativePath)
    let mihomo = try requireBundlePayload(stagedApp, relativePath: mihomoRelativePath)
    guard try sha256(helper) == expectedHelperHash,
        try sha256(mihomo) == expectedMihomoHash
    else {
        throw InstallerError.invalidSource("packaged payload changed during authorization")
    }
    let parentRequirement = try ManisHelperSecurity.pinnedRequirement(
        at: stagedApp,
        identifier: parentIdentifier
    )
    guard parentRequirement == expectedAppRequirement else {
        throw InstallerError.invalidSource("Manis.app changed after administrator authorization")
    }
    let clientRequirement = try ManisHelperSecurity.pinnedRequirement(
        at: helperControl,
        identifier: helperControlIdentifier
    )
    let helperRequirement = try ManisHelperSecurity.pinnedRequirement(
        at: helper,
        identifier: helperIdentifier
    )

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
    try installPlist(
        allowedUser: allowedUser,
        clientRequirement: clientRequirement,
        parentRequirement: parentRequirement,
        helperRequirement: helperRequirement,
        approvedSeedHash: expectedMihomoHash
    )
    _ = try run("/bin/launchctl", ["bootstrap", "system", installedPlist])
    _ = try run("/bin/launchctl", ["enable", "system/\(localServiceLabel)"])
    _ = try run("/bin/launchctl", ["kickstart", "-k", "system/\(localServiceLabel)"])
    print("registered local TUN helper")
}

do {
    guard CommandLine.arguments.count == 7,
        CommandLine.arguments[1] == "reinstall",
        let allowedUser = uid_t(CommandLine.arguments[6])
    else {
        throw InstallerError.invalidInvocation
    }
    try reinstall(
        appPath: CommandLine.arguments[2],
        expectedAppRequirement: CommandLine.arguments[3],
        expectedHelperHash: CommandLine.arguments[4],
        expectedMihomoHash: CommandLine.arguments[5],
        allowedUser: allowedUser
    )
} catch {
    fputs("\(error)\n", stderr)
    Foundation.exit(1)
}

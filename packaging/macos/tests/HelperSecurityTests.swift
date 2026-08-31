// This executable uses real ad-hoc signatures and child processes; it never requests root access.
let work = URL(fileURLWithPath: CommandLine.arguments[1])
let approved = work.appendingPathComponent("approved/Manis.app")
let forged = work.appendingPathComponent("forged/Manis.app")
var checks = 0

func check(_ condition: Bool, _ message: String) {
    guard condition else { fatalError(message) }
    checks += 1
}

func rejects(_ message: String, _ operation: () throws -> Void) {
    do { try operation() } catch { checks += 1; return }
    fatalError(message)
}

func parentProbe(_ app: URL, requirement: String) throws -> Int32 {
    let process = Process()
    process.executableURL = app.appendingPathComponent("Contents/MacOS/Manis")
    process.arguments = [work.appendingPathComponent("probe").path, app.path, requirement]
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    try process.run()
    process.waitUntilExit()
    return process.terminationStatus
}

let requirement = try ManisHelperSecurity.pinnedRequirement(at: approved, identifier: "dev.manis.app")
check(ManisHelperSecurity.validatePinnedRequirement(requirement, identifier: "dev.manis.app"), "valid pin rejected")
_ = try ManisHelperSecurity.verifiedCode(at: approved, requirement: requirement)
check(try parentProbe(approved, requirement: requirement) == 0, "approved parent could not launch controller")
rejects("same-ID forged bundle passed approval") {
    _ = try ManisHelperSecurity.verifiedCode(at: forged, requirement: requirement)
}
check(try parentProbe(forged, requirement: requirement) != 0, "same-ID forged parent launched controller")
rejects("wrong code identifier accepted") {
    _ = try ManisHelperSecurity.pinnedRequirement(at: approved, identifier: "dev.manis.app.helperctl")
}

let hash = String(repeating: "a", count: 40)
let pin: (String) -> String = { "identifier \"\($0)\" and cdhash H\"\(hash)\"" }
let environment = [
    "MANIS_ADMINISTRATOR_INSTALL": "1",
    "MANIS_LOCAL_ALLOWED_UID": "501",
    "MANIS_REQUIRED_PARENT_REQUIREMENT": pin("dev.manis.app"),
    "MANIS_REQUIRED_CLIENT_REQUIREMENT": pin("dev.manis.app.helperctl"),
    "MANIS_REQUIRED_HELPER_REQUIREMENT": pin("dev.manis.app.helper"),
    "MANIS_APPROVED_SEED_SHA256": String(repeating: "b", count: 64),
]
_ = try ManisHelperSecurity.Approval(environment: environment)
for key in environment.keys {
    var missing = environment
    missing.removeValue(forKey: key)
    rejects("incomplete policy accepted: \(key)") { _ = try ManisHelperSecurity.Approval(environment: missing) }
}
for unsafe in ["identifier \"dev.manis.app.helperctl\"", pin("dev.manis.app.helperctl") + " or true", pin("other.app") ] {
    var malformed = environment
    malformed["MANIS_REQUIRED_CLIENT_REQUIREMENT"] = unsafe
    rejects("broad or foreign policy accepted") { _ = try ManisHelperSecurity.Approval(environment: malformed) }
}
var rootUser = environment
rootUser["MANIS_LOCAL_ALLOWED_UID"] = "0"
rejects("root UID accepted for a client approval") { _ = try ManisHelperSecurity.Approval(environment: rootUser) }
var invalidSeed = environment
invalidSeed["MANIS_APPROVED_SEED_SHA256"] = "untrusted"
rejects("invalid seed digest accepted") { _ = try ManisHelperSecurity.Approval(environment: invalidSeed) }

let trustedDigest = SHA256.hash(data: Data("trusted core".utf8)).map { String(format: "%02x", $0) }.joined()
check(
    try MihomoReleaseVerifier.unpackedGzipSha256(Data(contentsOf: work.appendingPathComponent("core.gz"))) == trustedDigest,
    "release archive digest is incorrect"
)
rejects("invalid compressed release accepted") {
    _ = try MihomoReleaseVerifier.unpackedGzipSha256(Data("not gzip".utf8))
}
rejects("oversized expanded release accepted") {
    _ = try MihomoReleaseVerifier.unpackedGzipSha256(Data(contentsOf: work.appendingPathComponent("oversized.gz")))
}
rejects("non-HTTPS release source accepted") {
    _ = try MihomoReleaseVerifier.downloadHTTPS(URL(string: "http://invalid.example")!, maximumBytes: 1024)
}

let policy = work.appendingPathComponent("untrusted.plist")
try Data("policy".utf8).write(to: policy)
rejects("user-writable approval accepted") {
    try ManisHelperSecurity.requireRootOwnedPath(policy.path, directory: false)
}
let link = work.appendingPathComponent("policy-link")
try FileManager.default.createSymbolicLink(at: link, withDestinationURL: policy)
rejects("symlink policy accepted") {
    try ManisHelperSecurity.requireRootOwnedPath(link.path, directory: false)
}
try Data("tampered seed".utf8).write(to: approved.appendingPathComponent("Contents/Resources/identity.txt"))
rejects("tampered bundle resources accepted") {
    _ = try ManisHelperSecurity.pinnedRequirement(at: approved, identifier: "dev.manis.app")
}
print("helper security: \(checks) checks passed")

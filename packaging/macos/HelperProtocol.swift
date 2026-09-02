import Darwin
import Foundation

let manisHelperProtocolVersion = "v9"

func validateMihomoStop(
    childOwner: uid_t?,
    actualPid: pid_t?,
    owner: uid_t,
    expectedPid: pid_t
) -> String? {
    guard expectedPid > 0 else {
        return "Mihomo pid is invalid"
    }
    guard let actualPid else {
        return nil
    }
    guard childOwner == owner else {
        return "Mihomo is owned by another user"
    }
    guard actualPid == expectedPid else {
        return "Mihomo pid mismatch: expected \(expectedPid), running \(actualPid)"
    }
    return nil
}

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

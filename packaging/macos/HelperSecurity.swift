import Darwin
import Foundation
import Security

// Shared by the installer and controller. Approval pins code, not a freely chosen bundle ID.
enum ManisHelperSecurity {
    static let administratorPlist = "/Library/LaunchDaemons/dev.manis.app.helper.local.plist"

    struct Failure: Error, CustomStringConvertible {
        let description: String
        init(_ description: String) { self.description = description }
    }

    struct Approval {
        let parent: String
        let client: String
        let helper: String
        let seedSHA256: String
        let user: uid_t

        init(environment: [String: String]) throws {
            guard environment["MANIS_ADMINISTRATOR_INSTALL"] == "1",
                let parent = environment["MANIS_REQUIRED_PARENT_REQUIREMENT"],
                let client = environment["MANIS_REQUIRED_CLIENT_REQUIREMENT"],
                let helper = environment["MANIS_REQUIRED_HELPER_REQUIREMENT"],
                let seedSHA256 = environment["MANIS_APPROVED_SEED_SHA256"],
                let user = environment["MANIS_LOCAL_ALLOWED_UID"].flatMap(uid_t.init), user > 0,
                validatePinnedRequirement(parent, identifier: "dev.manis.app"),
                validatePinnedRequirement(client, identifier: "dev.manis.app.helperctl"),
                validatePinnedRequirement(helper, identifier: "dev.manis.app.helper"),
                seedSHA256.utf8.count == 64,
                seedSHA256.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) })
            else {
                throw Failure("Manis TUN approval is missing or outdated; administrator authorization is required")
            }
            self.parent = parent
            self.client = client
            self.helper = helper
            self.seedSHA256 = seedSHA256
            self.user = user
        }
    }

    static func validatePinnedRequirement(_ requirement: String, identifier: String) -> Bool {
        let prefix = "identifier \"\(identifier)\" and cdhash H\""
        guard requirement.hasPrefix(prefix), requirement.hasSuffix("\"") else { return false }
        let hash = requirement.dropFirst(prefix.count).dropLast()
        return hash.utf8.count == 40 && hash.utf8.allSatisfy {
            (48...57).contains($0) || (97...102).contains($0)
        }
    }

    static func requirement(_ text: String) throws -> SecRequirement {
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess,
            let requirement
        else { throw Failure("invalid Manis code-signing requirement") }
        return requirement
    }

    static func verifiedCode(at url: URL, requirement text: String? = nil) throws -> SecStaticCode {
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(url as CFURL, [], &code) == errSecSuccess, let code else {
            throw Failure("could not inspect Manis code signature")
        }
        let required = try text.map(requirement)
        let flags = SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckNestedCode)
        guard SecStaticCodeCheckValidity(code, flags, required) == errSecSuccess else {
            throw Failure("Manis code signature is invalid or the approved application has changed")
        }
        return code
    }

    static func pinnedRequirement(at url: URL, identifier: String) throws -> String {
        let code = try verifiedCode(at: url)
        var information: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &information)
            == errSecSuccess,
            let info = information as? [String: Any],
            info[kSecCodeInfoIdentifier as String] as? String == identifier,
            let hash = info[kSecCodeInfoUnique as String] as? Data, hash.count == 20
        else { throw Failure("unexpected Manis signing identity") }
        let hex = hash.map { String(format: "%02x", $0) }.joined()
        return "identifier \"\(identifier)\" and cdhash H\"\(hex)\""
    }

    static func requireRootOwnedPath(_ path: String, directory: Bool) throws {
        guard path.hasPrefix("/"), URL(fileURLWithPath: path).standardizedFileURL.path == path else {
            throw Failure("invalid Manis approval path")
        }
        var current = ""
        let components = path.split(separator: "/")
        for (index, component) in components.enumerated() {
            current += "/\(component)"
            var metadata = stat()
            let expectedType = index == components.count - 1 && !directory ? S_IFREG : S_IFDIR
            guard lstat(current, &metadata) == 0,
                metadata.st_mode & S_IFMT == expectedType,
                metadata.st_uid == 0, metadata.st_mode & 0o022 == 0
            else { throw Failure("unsafe or missing root-owned Manis approval path") }
        }
    }

    static func installedApproval() throws -> Approval {
        try requireRootOwnedPath(administratorPlist, directory: false)
        let data = try Data(contentsOf: URL(fileURLWithPath: administratorPlist))
        guard data.count <= 64 * 1024,
            let plist = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
            plist["Label"] as? String == "dev.manis.app.helper.local",
            let environment = plist["EnvironmentVariables"] as? [String: String]
        else { throw Failure("invalid Manis TUN approval") }
        return try Approval(environment: environment)
    }

    static func validateParent(bundle: URL, requirement text: String) throws {
        _ = try verifiedCode(at: bundle, requirement: text)
        var parent: SecCode?
        let attributes = [kSecGuestAttributePid: NSNumber(value: getppid())] as CFDictionary
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &parent) == errSecSuccess,
            let parent,
            SecCodeCheckValidity(parent, SecCSFlags(rawValue: kSecCSStrictValidate), try requirement(text))
                == errSecSuccess
        else { throw Failure("manis-helperctl must be launched directly by the approved Manis.app") }
    }

    static func ownTeamRequirement(identifier: String) throws -> String {
        var ownCode: SecCode?
        var staticCode: SecStaticCode?
        var information: CFDictionary?
        guard SecCodeCopySelf([], &ownCode) == errSecSuccess, let ownCode,
            SecCodeCopyStaticCode(ownCode, [], &staticCode) == errSecSuccess, let staticCode,
            SecCodeCopySigningInformation(staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &information)
                == errSecSuccess,
            let info = information as? [String: Any],
            let team = info[kSecCodeInfoTeamIdentifier as String] as? String,
            team.utf8.count == 10,
            team.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) })
        else { throw Failure("signed Manis helper controller has no valid developer Team ID") }
        return "identifier \"\(identifier)\" and anchor apple generic and certificate leaf[subject.OU] = \"\(team)\""
    }
}

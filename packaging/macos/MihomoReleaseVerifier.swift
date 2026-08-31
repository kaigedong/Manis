import CryptoKit
import Darwin
import Foundation

enum MihomoReleaseVerifier {
    private static let maximumCoreBytes = 128 * 1024 * 1024
    private static let maximumReleaseMetadataBytes = 1024 * 1024
    private static let maximumReleaseAssetBytes = 64 * 1024 * 1024
    private static let latestMihomoRelease = URL(
        string: "https://api.github.com/repos/MetaCubeX/mihomo/releases/latest"
    )!

    static func latestDigest() throws -> String {
        let data = try downloadHTTPS(latestMihomoRelease, maximumBytes: maximumReleaseMetadataBytes)
        guard
            let release = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            release["prerelease"] as? Bool == false,
            let tag = release["tag_name"] as? String,
            !tag.isEmpty,
            let assets = release["assets"] as? [[String: Any]]
        else {
            throw ManisHelperSecurity.Failure("could not verify the official latest Mihomo release")
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
            throw ManisHelperSecurity.Failure("unsupported macOS architecture for Mihomo")
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
                throw ManisHelperSecurity.Failure(
                    "official Mihomo release asset digest does not match"
                )
            }
            return try unpackedGzipSha256(archive)
        }
        throw ManisHelperSecurity.Failure("official Mihomo release has no trusted digest for this Mac")
    }

    static func downloadHTTPS(_ url: URL, maximumBytes: Int) throws -> Data {
        guard url.scheme == "https" else {
            throw ManisHelperSecurity.Failure("trusted Mihomo release URL must use HTTPS")
        }
        let process = Process()
        let output = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
        process.arguments = [
            "--disable",
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
            throw ManisHelperSecurity.Failure("could not download trusted Mihomo release data")
        }
        return data
    }

    static func unpackedGzipSha256(_ archive: Data) throws -> String {
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
            throw ManisHelperSecurity.Failure("official Mihomo release archive is invalid")
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

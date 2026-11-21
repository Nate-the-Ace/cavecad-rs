import Foundation
import Security

/// Secrets kept in the Keychain (this device only): the sign-in token, and this phone's device key.
///
/// The app is entitled to two access groups (Resources/CaveCADCapture.entitlements): its own
/// `<team>.app.cavecad.capture`, listed first, and `<team>.app.cavecad.shared`, where CaveCAD on this phone leaves
/// its stations (SharedStations.swift). Writes name the own group, so a secret never lands in the shared one; reads
/// and deletes do not filter by group, so items written before the entitlement existed are still found.
enum Keychain {
    static let token = "app.cavecad.capture.token"
    static let deviceKey = "app.cavecad.capture.device-key"

    /// "<TeamID>." from Info.plist (`AppIdentifierPrefix = $(AppIdentifierPrefix)`); nil in an unsigned build.
    static let prefix: String? = prefix(from: Bundle.main.object(forInfoDictionaryKey: "AppIdentifierPrefix") as? String)
    static let ownGroup: String? = group("app.cavecad.capture", prefix: prefix)
    static let sharedGroup: String? = group("app.cavecad.shared", prefix: prefix)

    static func prefix(from raw: String?) -> String? {
        guard let t = raw?.trimmingCharacters(in: .whitespaces), !t.isEmpty, !t.contains("$(") else { return nil }
        return t.hasSuffix(".") ? t : t + "."
    }

    static func group(_ suffix: String, prefix: String?) -> String? { prefix.map { $0 + suffix } }

    static func saveQuery(_ value: String, service: String, group: String?) -> [String: Any] {
        var q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
                                kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
                                kSecValueData as String: Data(value.utf8)]
        if let group { q[kSecAttrAccessGroup as String] = group }
        return q
    }

    static func loadQuery(service: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
         kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
    }

    static func deleteQuery(service: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service]
    }

    /// Keeps a value; false when the Keychain refused it.
    @discardableResult
    static func save(_ value: String, service: String = token) -> Bool {
        delete(service: service)
        var st = SecItemAdd(saveQuery(value, service: service, group: ownGroup) as CFDictionary, nil)
        if st == errSecMissingEntitlement, ownGroup != nil {
            // a build signed without the groups: the default group, as before
            Log.write("account", "keychain group not entitled; saving in the default group")
            st = SecItemAdd(saveQuery(value, service: service, group: nil) as CFDictionary, nil)
        }
        return st == errSecSuccess
    }

    static func load(service: String = token) -> String? {
        var out: CFTypeRef?
        guard SecItemCopyMatching(loadQuery(service: service) as CFDictionary, &out) == errSecSuccess, let d = out as? Data else { return nil }
        return String(data: d, encoding: .utf8)
    }

    static func delete(service: String = token) {
        SecItemDelete(deleteQuery(service: service) as CFDictionary)
    }
}

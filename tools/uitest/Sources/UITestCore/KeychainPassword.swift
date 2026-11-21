import Foundation

public func resolveKeychainPassword(environment: [String: String], stored: () -> String?) -> String? {
    if let value = environment["UITEST_KEYCHAIN_PASSWORD"], !value.isEmpty { return value }
    if environment["UITEST_NO_STORED_PASSWORD"] == "1" { return nil }
    return stored()
}

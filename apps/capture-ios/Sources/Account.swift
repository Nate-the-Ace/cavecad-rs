import AuthenticationServices
import CryptoKit
import Foundation

enum PKCE {
    static func verifier() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return base64url(Data(bytes))
    }
    static func challenge(for verifier: String) -> String {
        base64url(Data(SHA256.hash(data: Data(verifier.utf8))))
    }
    static func base64url(_ d: Data) -> String {
        d.base64EncodedString().replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }
}

/// The CaveCAD account: sign-in through cavecad.com (the desktop hand-off) and the bearer token.
@MainActor
final class Account: NSObject, ObservableObject, ASWebAuthenticationPresentationContextProviding {
    static let server = URL(string: Bundle.main.object(forInfoDictionaryKey: "CaveCADServer") as? String ?? "https://cavecad.com")!

    @Published private(set) var token: String? = Keychain.load()
    @Published private(set) var email: String?
    @Published var problem: String?
    /// Held while the sign-in sheet is up; a released session can end early.
    private var authSession: ASWebAuthenticationSession?

    nonisolated static func signInURL(server: URL, challenge: String, state: String) -> URL {
        var c = URLComponents(url: server.appendingPathComponent("signin"), resolvingAgainstBaseURL: false)!
        c.queryItems = [.init(name: "desktop", value: "1"), .init(name: "challenge", value: challenge), .init(name: "state", value: state)]
        return c.url!
    }

    nonisolated static func code(from callback: URL, state: String) -> String? {
        let q = URLComponents(url: callback, resolvingAgainstBaseURL: false)?.queryItems ?? []
        guard q.first(where: { $0.name == "state" })?.value == state else { return nil }
        return q.first(where: { $0.name == "code" })?.value
    }

    func signIn() {
        let verifier = PKCE.verifier(), state = PKCE.verifier()
        let url = Self.signInURL(server: Self.server, challenge: PKCE.challenge(for: verifier), state: state)
        let session = ASWebAuthenticationSession(url: url, callback: .customScheme("cavecad")) { [weak self] cb, error in
            Task { @MainActor in
                guard let self else { return }
                self.authSession = nil
                guard let cb, let code = Self.code(from: cb, state: state) else {
                    self.problem = error == nil ? "Sign-in did not finish." : nil
                    return
                }
                await self.redeem(code: code, verifier: verifier)
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        authSession = session
        session.start()
    }

    private func redeem(code: String, verifier: String) async {
        var req = URLRequest(url: Self.server.appendingPathComponent("api/desktop-token"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: ["code": code, "verifier": verifier])
        do {
            let (data, resp) = try await URLSession.shared.data(for: req)
            guard (resp as? HTTPURLResponse)?.statusCode == 200,
                  let t = (try JSONSerialization.jsonObject(with: data) as? [String: Any])?["token"] as? String
            else { problem = "The server refused the sign-in."; return }
            Keychain.save(t)
            token = t
            await loadMe()
        } catch { problem = "No connection to CaveCAD." }
    }

    func loadMe() async {
        guard let data = try? await get("api/me", device: false),
              let me = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        email = (me["member"] as? [String: Any])?["email"] as? String ?? me["email"] as? String
    }

    func signOut() { Keychain.delete(); token = nil; email = nil; deviceId = nil }

    /// This phone's id as a CaveCAD device. The server's cave calls all name one (`X-Cave-Device`); it is
    /// registered from an X25519 key kept in the Keychain, so the phone stays the same device across launches.
    private var deviceId: String?

    private func device() async throws -> String {
        if let deviceId { return deviceId }
        let key: Curve25519.KeyAgreement.PrivateKey
        if let raw = Keychain.load(service: Keychain.deviceKey).flatMap({ Data(base64Encoded: $0) }),
           let k = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: raw) {
            key = k
        } else {
            key = Curve25519.KeyAgreement.PrivateKey()
            Keychain.save(key.rawRepresentation.base64EncodedString(), service: Keychain.deviceKey)
        }
        var req = URLRequest(url: Self.server.appendingPathComponent("api/devices"))
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("Bearer \(token ?? "")", forHTTPHeaderField: "Authorization")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["publicKey": PKCE.base64url(key.publicKey.rawRepresentation)])
        let data = try await checked(URLSession.shared.data(for: req))
        guard let id = ((try JSONSerialization.jsonObject(with: data) as? [String: Any])?["device"] as? [String: Any])?["id"] as? String
        else { throw URLError(.badServerResponse) }
        deviceId = id
        return id
    }

    /// A request to the server with the bearer token and this phone's device id.
    func authorized(_ path: String, query: [URLQueryItem] = [], device needsDevice: Bool = true) async throws -> URLRequest {
        var c = URLComponents(url: Self.server.appendingPathComponent(path), resolvingAgainstBaseURL: false)!
        if !query.isEmpty { c.queryItems = query }
        var req = URLRequest(url: c.url!)
        req.setValue("Bearer \(token ?? "")", forHTTPHeaderField: "Authorization")
        if needsDevice { req.setValue(try await device(), forHTTPHeaderField: "X-Cave-Device") }
        return req
    }

    /// An authenticated GET. Throws on no connection or a refusal; a 401 signs out.
    func get(_ path: String, query: [URLQueryItem] = [], device: Bool = true) async throws -> Data {
        try await checked(URLSession.shared.data(for: authorized(path, query: query, device: device)))
    }

    private func checked(_ r: (Data, URLResponse)) throws -> Data {
        let status = (r.1 as? HTTPURLResponse)?.statusCode ?? 0
        if status == 401 { signOut() }
        guard (200..<300).contains(status) else { throw URLError(.badServerResponse) }
        return r.0
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first ?? ASPresentationAnchor()
        }
    }
}

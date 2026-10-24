# TinyAuth additions for CaveCAD

TinyAuth v5 on the edge (/opt/edge). Key names below are from tinyauth.app/docs/reference/configuration;
write them in the form the existing compose file uses (environment variables or the config file).

- Google sign-in: provider id `google`, `clientId`, `clientSecret` from a Google Cloud OAuth client
  (Web application) under cavecad.app@gmail.com. Authorized redirect URI: the one TinyAuth's docs give
  for Google on https://auth.schonegg.stream.
- Microsoft sign-in: a custom provider `microsoft` with
  `authUrl` https://login.microsoftonline.com/common/oauth2/v2.0/authorize,
  `tokenUrl` https://login.microsoftonline.com/common/oauth2/v2.0/token,
  `userinfoUrl` https://graph.microsoft.com/oidc/userinfo, `scopes` `openid email profile`,
  from an Entra app registration (any account type, personal included) under cavecad.app.
- No OAuth whitelist: anyone may prove who they are; CaveCAD decides who gets in.
- Verified email only. CaveCAD trusts the email TinyAuth passes on, and an invite is bound to that email.
  Google's email is verified. Microsoft's `email` claim can be set by the owner of a work tenant without
  proof (the "nOAuth" problem), so map TinyAuth's Microsoft email claim to one Microsoft verifies
  (`xms_edov`-checked email, or personal accounts only via the `consumers` endpoints) and test with a work
  account before go-live. Until that is settled, launch with Google only and leave Microsoft off.
- `auth.sessionExpiry`: 2592000 (30 days).
- OIDC client for the desktop app: `oidc.clients.cavecad-desktop` with `clientId` `cavecad-desktop`, a
  `clientSecretFile`, `trustedRedirectUris` `cavecad://auth`; `oidc.privateKeyPath` / `publicKeyPath` set.
- Check whether TinyAuth reports which provider someone used (a claim or `Remote-Sub` prefix). If it does,
  map it to `provider` in `apps/server/src/identity.ts`; if not, leave provider empty (the page hides it).

## As deployed (2026-10-24, TinyAuth v5.2.0, environment variables in /opt/edge/compose.yaml)

    TINYAUTH_AUTH_SESSIONEXPIRY=2592000
    TINYAUTH_OAUTH_PROVIDERS_GOOGLE_CLIENTID=<from /opt/edge/tinyauth/google-client-id>
    TINYAUTH_OAUTH_PROVIDERS_GOOGLE_CLIENTSECRETFILE=/data/google-client-secret
    TINYAUTH_OAUTH_PROVIDERS_GOOGLE_WHITELIST=/.*/
    TINYAUTH_APPS_CAVE_CONFIG_DOMAIN=cave.schonegg.stream
    TINYAUTH_APPS_CAVE_OAUTH_WHITELIST=/.*/
    TINYAUTH_APPS_CAVE_PATH_ALLOW=^/privacy\.html$$

The provider whitelist is needed: under the `deny` policy an empty one refuses every Google account at sign-in ("Email not whitelisted"). Then an OAuth user is let in only where an app's OAuth whitelist matches
(an empty whitelist abstains, so deny), so Google accounts reach cave. and nothing else; local users are
refused on cave. (empty users allow list). One TinyAuth session per browser covers every site.
`/opt/edge/save-google-client` (root, 700) saves a new Google client id and secret, the secret typed hidden.
Google project `cavecad` under cavecad.app@gmail.com, client "TinyAuth", in production, no logo yet
(a logo needs Google's brand review). Desktop sign-in (2026-10-24):

    TINYAUTH_OIDC_PRIVATEKEYPATH=/data/oidc_key
    TINYAUTH_OIDC_PUBLICKEYPATH=/data/oidc_key.pub
    TINYAUTH_OIDC_CLIENTS_CAVECAD_CLIENTID=cavecad-desktop
    TINYAUTH_OIDC_CLIENTS_CAVECAD_CLIENTSECRETFILE=/data/cavecad-desktop-secret
    TINYAUTH_OIDC_CLIENTS_CAVECAD_TRUSTEDREDIRECTURIS=cavecad://auth
    TINYAUTH_OIDC_CLIENTS_CAVECAD_NAME=CaveCAD desktop

The key pair is made by TinyAuth on first start at those paths (kept in /data so restarts keep it). The client
secret is the same value as the server's OIDC_CLIENT_SECRET, made on the container and copied over unseen.
TinyAuth v5.2.0 supports PKCE S256 (it does not advertise it in discovery), client_secret_post, and refresh
tokens; ID tokens are RS256 and last the session expiry (30 days). Any TinyAuth user may get a token: CaveCAD's
member list decides, as on the web.

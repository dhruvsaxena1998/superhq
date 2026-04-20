// Credential storage — WebAuthn PRF only.
//
// The device_key issued during pairing is the HMAC secret the client
// sends on every session.hello. We never let it touch plain storage:
// the browser wraps it with an AES-GCM key derived from a WebAuthn PRF
// output, which means decryption requires a user-verification gesture
// bound to a platform authenticator (Touch ID, Windows Hello, or a
// FIDO2 key with hmac-secret).
//
// No fallback. If the browser can't do WebAuthn-PRF, pairing fails and
// the user is told why.
//
// Schema: one IndexedDB object store keyed by peer endpoint id. Each
// record: { peerId, credentialId, device_id, ciphertext, nonce }.

const DB_NAME = "superhq-remote";
const DB_VERSION = 1;
const STORE = "credentials";

// Fixed per-peer input to the PRF extension. Changing this string
// invalidates every stored credential's encryption key — do not modify
// without a migration path.
function prfSalt(peerId) {
    return new TextEncoder().encode(`superhq.prf.v1.${peerId}`);
}

function rpId() {
    // Either the exact hostname or a registrable suffix of it.
    // "localhost" is specifically allowed by WebAuthn for dev.
    return window.location.hostname;
}

function b64encode(bytes) {
    const arr = new Uint8Array(bytes);
    let s = "";
    for (const b of arr) s += String.fromCharCode(b);
    return btoa(s);
}

function b64decode(str) {
    const s = atob(str);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
}

// ── IndexedDB wrapper ────────────────────────────────────────────────

function openDb() {
    return new Promise((resolve, reject) => {
        const req = indexedDB.open(DB_NAME, DB_VERSION);
        req.onupgradeneeded = () => {
            const db = req.result;
            if (!db.objectStoreNames.contains(STORE)) {
                db.createObjectStore(STORE, { keyPath: "peerId" });
            }
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
    });
}

async function dbGet(peerId) {
    const db = await openDb();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE, "readonly");
        const req = tx.objectStore(STORE).get(peerId);
        req.onsuccess = () => resolve(req.result || null);
        req.onerror = () => reject(req.error);
    });
}

async function dbPut(record) {
    const db = await openDb();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE, "readwrite");
        tx.objectStore(STORE).put(record);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
    });
}

async function dbDelete(peerId) {
    const db = await openDb();
    return new Promise((resolve, reject) => {
        const tx = db.transaction(STORE, "readwrite");
        tx.objectStore(STORE).delete(peerId);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
    });
}

// ── WebAuthn + PRF ───────────────────────────────────────────────────

function requireWebAuthn() {
    if (!window.isSecureContext) {
        throw new Error(
            "WebAuthn requires a secure context (HTTPS or localhost).",
        );
    }
    if (!window.PublicKeyCredential) {
        throw new Error("This browser does not expose WebAuthn.");
    }
}

async function createWebAuthnCredential(peerId) {
    const publicKey = {
        challenge: crypto.getRandomValues(new Uint8Array(32)),
        rp: { name: "SuperHQ Remote Control", id: rpId() },
        user: {
            id: crypto.getRandomValues(new Uint8Array(16)),
            name: `superhq:${peerId.slice(0, 16)}`,
            displayName: `SuperHQ host ${peerId.slice(0, 8)}…`,
        },
        pubKeyCredParams: [
            { type: "public-key", alg: -7 },   // ES256
            { type: "public-key", alg: -257 }, // RS256
        ],
        authenticatorSelection: {
            residentKey: "preferred",
            userVerification: "required",
        },
        extensions: {
            prf: { eval: { first: prfSalt(peerId) } },
        },
        timeout: 60_000,
    };
    const cred = await navigator.credentials.create({ publicKey });
    if (!cred) throw new Error("WebAuthn create returned null");
    const ext = cred.getClientExtensionResults();
    if (!ext?.prf?.enabled) {
        const authType = cred.response?.authenticatorAttachment
            || "unknown";
        console.warn("WebAuthn ext results:", ext);
        throw new Error(
            `PRF not enabled by authenticator (attachment=${authType}). ` +
            `This usually means your browser / OS / passkey provider does not support ` +
            `WebAuthn PRF yet. Try: Chrome 132+ on macOS 14+ using Touch ID, ` +
            `Chrome 116+ on Windows using Windows Hello, or a FIDO2 hardware ` +
            `key with hmac-secret (e.g. YubiKey 5).`,
        );
    }

    // If the authenticator returned PRF output at create time we use it
    // directly — one prompt, not two. Google Password Manager and Chrome
    // on macOS (132+) both do. Otherwise fall back to an immediate
    // get() to fetch it.
    const firstFromCreate = ext.prf.results?.first;
    const prfOut = firstFromCreate
        ? new Uint8Array(firstFromCreate)
        : await getWebAuthnPrfKey(peerId, cred.rawId);
    return { credentialId: new Uint8Array(cred.rawId), prfKey: prfOut };
}

async function getWebAuthnPrfKey(peerId, credentialIdBytes) {
    const publicKey = {
        challenge: crypto.getRandomValues(new Uint8Array(32)),
        rpId: rpId(),
        allowCredentials: [{
            type: "public-key",
            id: credentialIdBytes,
            transports: ["internal", "hybrid", "usb", "nfc", "ble"],
        }],
        userVerification: "required",
        extensions: {
            prf: { eval: { first: prfSalt(peerId) } },
        },
        timeout: 60_000,
    };
    const assertion = await navigator.credentials.get({ publicKey });
    if (!assertion) throw new Error("WebAuthn get returned null");
    const ext = assertion.getClientExtensionResults();
    const first = ext?.prf?.results?.first;
    if (!first) {
        throw new Error("authenticator did not return PRF output");
    }
    return new Uint8Array(first);
}

async function deriveAesKey(prfBytes) {
    return crypto.subtle.importKey(
        "raw",
        prfBytes,
        { name: "AES-GCM" },
        false,
        ["encrypt", "decrypt"],
    );
}

async function encryptDeviceKey(prfBytes, deviceKeyB64) {
    const key = await deriveAesKey(prfBytes);
    const nonce = crypto.getRandomValues(new Uint8Array(12));
    const plain = new TextEncoder().encode(deviceKeyB64);
    const ct = new Uint8Array(
        await crypto.subtle.encrypt({ name: "AES-GCM", iv: nonce }, key, plain),
    );
    return { ciphertext: ct, nonce };
}

async function decryptDeviceKey(prfBytes, ciphertext, nonce) {
    const key = await deriveAesKey(prfBytes);
    const plain = new Uint8Array(
        await crypto.subtle.decrypt(
            { name: "AES-GCM", iv: nonce },
            key,
            ciphertext,
        ),
    );
    return new TextDecoder().decode(plain);
}

// ── Public API ───────────────────────────────────────────────────────

// Session-scoped plaintext cache keyed by peerId. Populated after the
// first successful unlock (or right after pair), consulted by
// loadCredential so subsequent RPCs in the same tab don't each trigger
// a WebAuthn prompt. Evicted on clear and naturally gone on tab close.
const sessionCache = new Map();

/// Returns `{ device_id }` when a record exists, `null` otherwise.
/// Cheap — does not prompt for WebAuthn.
export async function describeCredential(peerId) {
    if (!peerId) return null;
    const row = await dbGet(peerId);
    return row ? { device_id: row.device_id } : null;
}

export async function saveCredential(peerId, { device_id, device_key }) {
    requireWebAuthn();
    const { credentialId, prfKey } = await createWebAuthnCredential(peerId);
    const { ciphertext, nonce } = await encryptDeviceKey(prfKey, device_key);
    await dbPut({
        peerId,
        credentialId: b64encode(credentialId),
        device_id,
        ciphertext: b64encode(ciphertext),
        nonce: b64encode(nonce),
    });
    // We just registered; stash the plaintext so the post-pair
    // session.hello doesn't immediately re-prompt for unlock.
    sessionCache.set(peerId, { device_id, device_key });
}

export async function loadCredential(peerId) {
    const cached = sessionCache.get(peerId);
    if (cached) return { ...cached };
    const row = await dbGet(peerId);
    if (!row) return null;
    requireWebAuthn();
    const prfKey = await getWebAuthnPrfKey(
        peerId,
        b64decode(row.credentialId),
    );
    const device_key = await decryptDeviceKey(
        prfKey,
        b64decode(row.ciphertext),
        b64decode(row.nonce),
    );
    const unlocked = { device_id: row.device_id, device_key };
    sessionCache.set(peerId, unlocked);
    return { ...unlocked };
}

export async function clearCredential(peerId) {
    sessionCache.delete(peerId);
    await dbDelete(peerId);
}

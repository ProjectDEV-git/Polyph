// Browser storage for the phone's room member: IndexedDB, with the identity
// and the room state sealed by an AES-GCM key that cannot be exported from
// the browser (WebCrypto, `extractable: false`). Reading the storage files
// off the device does not reveal room keys; code running in this origin can
// still use them, which is the browser's limit.

const DB = "polyph-member";
const STORE = "kv";

function open(): Promise<IDBDatabase> {
  return new Promise((ok, no) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => r.result.createObjectStore(STORE);
    r.onsuccess = () => ok(r.result);
    r.onerror = () => no(r.error);
  });
}

async function tx<T>(
  mode: IDBTransactionMode,
  f: (s: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  const db = await open();
  return new Promise((ok, no) => {
    const t = db.transaction(STORE, mode);
    const req = f(t.objectStore(STORE));
    req.onsuccess = () => ok(req.result);
    req.onerror = () => no(req.error);
  });
}

const get = <T>(k: string) =>
  tx<T | undefined>("readonly", (s) => s.get(k) as IDBRequest<T | undefined>);
const put = (k: string, v: unknown) => tx("readwrite", (s) => s.put(v, k));
const del = (k: string) => tx("readwrite", (s) => s.delete(k));

async function key(): Promise<CryptoKey> {
  const have = await get<CryptoKey>("key");
  if (have) return have;
  const k = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, [
    "encrypt",
    "decrypt",
  ]);
  await put("key", k);
  return k;
}

async function seal(data: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-GCM", iv }, await key(), data),
  );
  const out = new Uint8Array(12 + ct.length);
  out.set(iv);
  out.set(ct, 12);
  return out.buffer;
}

async function unseal(b: ArrayBuffer): Promise<Uint8Array> {
  const all = new Uint8Array(b);
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: all.slice(0, 12) },
    await key(),
    all.slice(12),
  );
  return new Uint8Array(pt);
}

export interface Profile {
  name: string;
  relay: string;
}

export const memberStorage = {
  async identity(make: () => Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
    const b = await get<ArrayBuffer>("identity");
    if (b) return unseal(b);
    const id = make();
    await put("identity", await seal(id));
    return id;
  },
  async room(): Promise<Uint8Array | null> {
    const b = await get<ArrayBuffer>("room");
    return b ? unseal(b) : null;
  },
  async saveRoom(data: Uint8Array<ArrayBuffer>): Promise<void> {
    await put("room", await seal(data));
  },
  async profile(): Promise<Profile | null> {
    return (await get<Profile>("profile")) ?? null;
  },
  async saveProfile(p: Profile): Promise<void> {
    await put("profile", p);
  },
  /** Leaves the room on this device: forgets its state and identity. */
  async forget(): Promise<void> {
    await del("room");
    await del("identity");
    await del("profile");
  },
};

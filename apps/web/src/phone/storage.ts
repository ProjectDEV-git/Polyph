// What the phone keeps between visits, in IndexedDB: its device secret and
// the engine's saved room, both sealed with AES-GCM under a key the browser
// generated as non-extractable. Script on this origin can still use the key,
// but a copy of the stored bytes alone (a backup, a synced profile, a disk
// image) cannot be opened without the browser's key store.

const DB = "polyph-phone";
const STORE = "kv";

function open(): Promise<IDBDatabase> {
  return new Promise((ok, no) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => r.result.createObjectStore(STORE);
    r.onsuccess = () => ok(r.result);
    r.onerror = () => no(r.error);
  });
}

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((ok, no) => {
    r.onsuccess = () => ok(r.result);
    r.onerror = () => no(r.error);
  });
}

export class Vault {
  private constructor(
    private db: IDBDatabase,
    private key: CryptoKey,
  ) {}

  static async open(): Promise<Vault> {
    const db = await open();
    const tx = db.transaction(STORE, "readonly");
    let key = (await req(tx.objectStore(STORE).get("key"))) as CryptoKey | undefined;
    if (!key) {
      key = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, [
        "encrypt",
        "decrypt",
      ]);
      await req(db.transaction(STORE, "readwrite").objectStore(STORE).put(key, "key"));
    }
    return new Vault(db, key);
  }

  async get(name: string): Promise<Uint8Array | null> {
    const v = (await req(this.db.transaction(STORE, "readonly").objectStore(STORE).get(name))) as
      { iv: Uint8Array; data: ArrayBuffer } | undefined;
    if (!v) return null;
    try {
      const pt = await crypto.subtle.decrypt(
        {
          name: "AES-GCM",
          iv: v.iv as BufferSource,
          additionalData: new TextEncoder().encode(name),
        },
        this.key,
        v.data,
      );
      return new Uint8Array(pt);
    } catch {
      return null;
    }
  }

  async put(name: string, bytes: Uint8Array): Promise<void> {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const data = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv, additionalData: new TextEncoder().encode(name) },
      this.key,
      bytes as BufferSource,
    );
    await req(this.db.transaction(STORE, "readwrite").objectStore(STORE).put({ iv, data }, name));
  }

  async remove(name: string): Promise<void> {
    await req(this.db.transaction(STORE, "readwrite").objectStore(STORE).delete(name));
  }
}

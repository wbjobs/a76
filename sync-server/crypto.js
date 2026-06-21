import crypto from 'crypto';

const ALGORITHM = 'aes-256-gcm';
const KEY_LENGTH = 32;
const NONCE_LENGTH = 12;
const TAG_LENGTH = 16;

export function deriveKeyFromPassword(password) {
  return crypto.pbkdf2Sync(password, 'memsnapshot_salt_v1', 100000, KEY_LENGTH, 'sha256');
}

export function encrypt(data, key) {
  const nonce = crypto.randomBytes(NONCE_LENGTH);
  const cipher = crypto.createCipheriv(ALGORITHM, key, nonce);
  
  const encrypted = Buffer.concat([
    cipher.update(JSON.stringify(data), 'utf8'),
    cipher.final()
  ]);
  
  const tag = cipher.getAuthTag();
  
  return Buffer.concat([nonce, tag, encrypted]).toString('base64');
}

export function decrypt(encryptedB64, key) {
  const buf = Buffer.from(encryptedB64, 'base64');
  
  const nonce = buf.subarray(0, NONCE_LENGTH);
  const tag = buf.subarray(NONCE_LENGTH, NONCE_LENGTH + TAG_LENGTH);
  const encrypted = buf.subarray(NONCE_LENGTH + TAG_LENGTH);
  
  const decipher = crypto.createDecipheriv(ALGORITHM, key, nonce);
  decipher.setAuthTag(tag);
  
  const decrypted = Buffer.concat([
    decipher.update(encrypted),
    decipher.final()
  ]);
  
  return JSON.parse(decrypted.toString('utf8'));
}

export function sha256(data) {
  return crypto.createHash('sha256').update(data).digest('hex');
}

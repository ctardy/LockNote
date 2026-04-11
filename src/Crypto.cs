using System;
using System.Security.Cryptography;
using System.Text;

namespace LockNote
{
    /// <summary>
    /// AES-256-CBC encryption with HMAC-SHA256 (encrypt-then-MAC).
    /// Key derivation: PBKDF2-SHA256, 100 000 iterations.
    /// Wire format: [salt 16][iv 16][hmac 32][ciphertext]
    /// All sensitive buffers are zeroed after use.
    /// </summary>
    static class Crypto
    {
        const int SaltSize = 16;
        const int IVSize = 16;
        const int HMACSize = 32;
        const int KeySize = 32;
        const int Iterations = 100000;

        public static byte[] Encrypt(string plaintext, string password)
        {
            byte[] salt = new byte[SaltSize];
            byte[] iv = new byte[IVSize];
            using (var rng = new RNGCryptoServiceProvider())
            {
                rng.GetBytes(salt);
                rng.GetBytes(iv);
            }

            byte[] keyMaterial = null;
            byte[] encKey = null;
            byte[] macKey = null;
            byte[] ciphertext = null;
            byte[] hmacValue = null;
            byte[] plainBytes = null;

            try
            {
                using (var kdf = new Rfc2898DeriveBytes(password, salt, Iterations, HashAlgorithmName.SHA256))
                {
                    keyMaterial = kdf.GetBytes(64);
                }
                encKey = new byte[KeySize];
                macKey = new byte[KeySize];
                Buffer.BlockCopy(keyMaterial, 0, encKey, 0, KeySize);
                Buffer.BlockCopy(keyMaterial, KeySize, macKey, 0, KeySize);

                plainBytes = Encoding.UTF8.GetBytes(plaintext);

                using (var aes = Aes.Create())
                {
                    aes.KeySize = 256;
                    aes.Mode = CipherMode.CBC;
                    aes.Padding = PaddingMode.PKCS7;
                    aes.Key = encKey;
                    aes.IV = iv;
                    using (var enc = aes.CreateEncryptor())
                    {
                        ciphertext = enc.TransformFinalBlock(plainBytes, 0, plainBytes.Length);
                    }
                }

                // HMAC covers salt + iv + ciphertext
                byte[] toMac = new byte[SaltSize + IVSize + ciphertext.Length];
                Buffer.BlockCopy(salt, 0, toMac, 0, SaltSize);
                Buffer.BlockCopy(iv, 0, toMac, SaltSize, IVSize);
                Buffer.BlockCopy(ciphertext, 0, toMac, SaltSize + IVSize, ciphertext.Length);

                using (var hmac = new HMACSHA256(macKey))
                {
                    hmacValue = hmac.ComputeHash(toMac);
                }
                Array.Clear(toMac, 0, toMac.Length);

                // [salt 16][iv 16][hmac 32][ciphertext]
                byte[] result = new byte[SaltSize + IVSize + HMACSize + ciphertext.Length];
                Buffer.BlockCopy(salt, 0, result, 0, SaltSize);
                Buffer.BlockCopy(iv, 0, result, SaltSize, IVSize);
                Buffer.BlockCopy(hmacValue, 0, result, SaltSize + IVSize, HMACSize);
                Buffer.BlockCopy(ciphertext, 0, result, SaltSize + IVSize + HMACSize, ciphertext.Length);
                return result;
            }
            finally
            {
                if (keyMaterial != null) Array.Clear(keyMaterial, 0, keyMaterial.Length);
                if (encKey != null) Array.Clear(encKey, 0, encKey.Length);
                if (macKey != null) Array.Clear(macKey, 0, macKey.Length);
                if (plainBytes != null) Array.Clear(plainBytes, 0, plainBytes.Length);
                if (ciphertext != null) Array.Clear(ciphertext, 0, ciphertext.Length);
                if (hmacValue != null) Array.Clear(hmacValue, 0, hmacValue.Length);
            }
        }

        /// <summary>
        /// Returns the decrypted plaintext, or null if the password is wrong
        /// (HMAC mismatch) or the data is corrupted.
        /// </summary>
        public static string Decrypt(byte[] data, string password)
        {
            if (data.Length < SaltSize + IVSize + HMACSize + 16)
                return null;

            byte[] salt = new byte[SaltSize];
            byte[] iv = new byte[IVSize];
            byte[] storedHmac = new byte[HMACSize];
            int ctLen = data.Length - SaltSize - IVSize - HMACSize;
            byte[] ciphertext = new byte[ctLen];

            Buffer.BlockCopy(data, 0, salt, 0, SaltSize);
            Buffer.BlockCopy(data, SaltSize, iv, 0, IVSize);
            Buffer.BlockCopy(data, SaltSize + IVSize, storedHmac, 0, HMACSize);
            Buffer.BlockCopy(data, SaltSize + IVSize + HMACSize, ciphertext, 0, ctLen);

            byte[] keyMaterial = null;
            byte[] encKey = null;
            byte[] macKey = null;
            byte[] plainBytes = null;

            try
            {
                using (var kdf = new Rfc2898DeriveBytes(password, salt, Iterations, HashAlgorithmName.SHA256))
                {
                    keyMaterial = kdf.GetBytes(64);
                }
                encKey = new byte[KeySize];
                macKey = new byte[KeySize];
                Buffer.BlockCopy(keyMaterial, 0, encKey, 0, KeySize);
                Buffer.BlockCopy(keyMaterial, KeySize, macKey, 0, KeySize);

                // Verify HMAC before decrypting (reject early on wrong password)
                byte[] toMac = new byte[SaltSize + IVSize + ctLen];
                Buffer.BlockCopy(salt, 0, toMac, 0, SaltSize);
                Buffer.BlockCopy(iv, 0, toMac, SaltSize, IVSize);
                Buffer.BlockCopy(ciphertext, 0, toMac, SaltSize + IVSize, ctLen);

                byte[] computedHmac;
                using (var hmac = new HMACSHA256(macKey))
                {
                    computedHmac = hmac.ComputeHash(toMac);
                }
                Array.Clear(toMac, 0, toMac.Length);

                if (!ConstantTimeEquals(storedHmac, computedHmac))
                {
                    Array.Clear(computedHmac, 0, computedHmac.Length);
                    return null;
                }
                Array.Clear(computedHmac, 0, computedHmac.Length);

                using (var aes = Aes.Create())
                {
                    aes.KeySize = 256;
                    aes.Mode = CipherMode.CBC;
                    aes.Padding = PaddingMode.PKCS7;
                    aes.Key = encKey;
                    aes.IV = iv;
                    using (var dec = aes.CreateDecryptor())
                    {
                        plainBytes = dec.TransformFinalBlock(ciphertext, 0, ciphertext.Length);
                    }
                }

                return Encoding.UTF8.GetString(plainBytes);
            }
            catch (CryptographicException)
            {
                return null;
            }
            finally
            {
                if (keyMaterial != null) Array.Clear(keyMaterial, 0, keyMaterial.Length);
                if (encKey != null) Array.Clear(encKey, 0, encKey.Length);
                if (macKey != null) Array.Clear(macKey, 0, macKey.Length);
                if (plainBytes != null) Array.Clear(plainBytes, 0, plainBytes.Length);
                Array.Clear(ciphertext, 0, ciphertext.Length);
            }
        }

        /// <summary>Constant-time byte array comparison to prevent timing attacks.</summary>
        static bool ConstantTimeEquals(byte[] a, byte[] b)
        {
            if (a.Length != b.Length) return false;
            int diff = 0;
            for (int i = 0; i < a.Length; i++)
                diff |= a[i] ^ b[i];
            return diff == 0;
        }
    }
}

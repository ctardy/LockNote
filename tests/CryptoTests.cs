using System;
using System.Text;

namespace LockNote.Tests
{
    static class CryptoTests
    {
        [Test]
        public static void EncryptDecrypt_RoundTrip()
        {
            string plaintext = "Hello, LockNote!";
            string password = "testpassword123";

            byte[] encrypted = Crypto.Encrypt(plaintext, password);
            string decrypted = Crypto.Decrypt(encrypted, password);

            Assert.AreEqual(plaintext, decrypted, "Round-trip should preserve plaintext");
        }

        [Test]
        public static void EncryptDecrypt_EmptyString()
        {
            string plaintext = "";
            string password = "pass";

            byte[] encrypted = Crypto.Encrypt(plaintext, password);
            string decrypted = Crypto.Decrypt(encrypted, password);

            Assert.AreEqual(plaintext, decrypted, "Empty string round-trip");
        }

        [Test]
        public static void EncryptDecrypt_Unicode()
        {
            string plaintext = "Héllo wörld! 日本語テスト 🔒";
            string password = "unicöde_pàss";

            byte[] encrypted = Crypto.Encrypt(plaintext, password);
            string decrypted = Crypto.Decrypt(encrypted, password);

            Assert.AreEqual(plaintext, decrypted, "Unicode round-trip");
        }

        [Test]
        public static void EncryptDecrypt_LargeText()
        {
            var sb = new StringBuilder();
            for (int i = 0; i < 10000; i++)
                sb.AppendLine("Line " + i + ": The quick brown fox jumps over the lazy dog.");
            string plaintext = sb.ToString();
            string password = "largetest";

            byte[] encrypted = Crypto.Encrypt(plaintext, password);
            string decrypted = Crypto.Decrypt(encrypted, password);

            Assert.AreEqual(plaintext, decrypted, "Large text round-trip");
        }

        [Test]
        public static void Decrypt_WrongPassword_ReturnsNull()
        {
            string plaintext = "secret data";
            byte[] encrypted = Crypto.Encrypt(plaintext, "correct");

            string result = Crypto.Decrypt(encrypted, "wrong");

            Assert.IsNull(result, "Wrong password should return null");
        }

        [Test]
        public static void Decrypt_TruncatedData_ReturnsNull()
        {
            // Data too short to contain salt+iv+hmac+ciphertext
            byte[] tooShort = new byte[60];
            string result = Crypto.Decrypt(tooShort, "pass");

            Assert.IsNull(result, "Truncated data should return null");
        }

        [Test]
        public static void Decrypt_CorruptedData_ReturnsNull()
        {
            byte[] encrypted = Crypto.Encrypt("test", "pass");

            // Corrupt the ciphertext area (after salt+iv+hmac = 64 bytes)
            if (encrypted.Length > 65)
                encrypted[65] ^= 0xFF;

            string result = Crypto.Decrypt(encrypted, "pass");

            Assert.IsNull(result, "Corrupted data should return null");
        }

        [Test]
        public static void Encrypt_ProducesDifferentCiphertexts()
        {
            // Each call should use a different salt and IV
            string plaintext = "same text";
            string password = "same pass";

            byte[] enc1 = Crypto.Encrypt(plaintext, password);
            byte[] enc2 = Crypto.Encrypt(plaintext, password);

            bool different = false;
            if (enc1.Length != enc2.Length)
            {
                different = true;
            }
            else
            {
                for (int i = 0; i < enc1.Length; i++)
                {
                    if (enc1[i] != enc2[i]) { different = true; break; }
                }
            }

            Assert.IsTrue(different, "Two encryptions of same data should differ (random salt/IV)");
        }

        [Test]
        public static void EncryptedData_HasCorrectMinimumSize()
        {
            byte[] encrypted = Crypto.Encrypt("x", "p");

            // salt(16) + iv(16) + hmac(32) + at least 16 bytes ciphertext (AES block)
            Assert.IsTrue(encrypted.Length >= 80,
                "Encrypted output should be at least 80 bytes, got " + encrypted.Length);
        }
    }
}

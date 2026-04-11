using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace LockNote
{
    /// <summary>
    /// Reads and writes encrypted data appended after a binary marker inside the .exe.
    /// The marker is reconstructed at runtime via XOR so the literal byte sequence
    /// never appears in the compiled binary (which would cause a false-positive match).
    /// </summary>
    static class Storage
    {
        static readonly byte[] MarkerXored = {
            0x4C ^ 0xAA, 0x4E ^ 0xAA, 0x5F ^ 0xAA, 0x44 ^ 0xAA,
            0x41 ^ 0xAA, 0x54 ^ 0xAA, 0x41 ^ 0xAA, 0x5F ^ 0xAA,
            0xDE ^ 0xAA, 0xAD ^ 0xAA, 0xBE ^ 0xAA, 0xEF ^ 0xAA,
            0xCA ^ 0xAA, 0xFE ^ 0xAA, 0xF0 ^ 0xAA, 0x0D ^ 0xAA
        };

        static byte[] BuildMarker()
        {
            byte[] m = new byte[16];
            for (int i = 0; i < 16; i++)
                m[i] = (byte)(MarkerXored[i] ^ 0xAA);
            return m;
        }

        static readonly byte[] Marker = BuildMarker();

        /// <summary>
        /// Returns a copy of the binary marker (used by Updater to migrate data).
        /// </summary>
        public static byte[] GetMarkerForUpdate()
        {
            byte[] copy = new byte[Marker.Length];
            Buffer.BlockCopy(Marker, 0, copy, 0, Marker.Length);
            return copy;
        }

        /// <summary>
        /// Returns the path to the .tmp staging file in %LOCALAPPDATA%\LockNote\.
        /// The filename is derived from a hash of the exe path so multiple instances
        /// don't collide. This keeps the .tmp hidden from the user.
        /// </summary>
        public static string GetTmpPath(string exePath)
        {
            string dir = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "LockNote");
            if (!Directory.Exists(dir))
                Directory.CreateDirectory(dir);

            // Short hash of the exe path for uniqueness
            byte[] pathBytes = Encoding.UTF8.GetBytes(exePath.ToUpperInvariant());
            byte[] hash;
            using (var sha = SHA256.Create())
            {
                hash = sha.ComputeHash(pathBytes);
            }
            string name = BitConverter.ToString(hash, 0, 8).Replace("-", "").ToLowerInvariant();
            return Path.Combine(dir, name + ".tmp");
        }

        /// <summary>
        /// Returns the encrypted payload appended after the marker, or null if none.
        /// </summary>
        public static byte[] ReadData(string filePath)
        {
            if (!File.Exists(filePath)) return null;

            byte[] data = File.ReadAllBytes(filePath);
            int pos = FindMarker(data);
            if (pos < 0 || pos + Marker.Length >= data.Length)
            {
                Array.Clear(data, 0, data.Length);
                return null;
            }

            int dataStart = pos + Marker.Length;
            int dataLen = data.Length - dataStart;

            // Minimum valid payload: salt(16) + iv(16) + hmac(32) + 16 bytes ciphertext
            if (dataLen < 80)
            {
                Array.Clear(data, 0, data.Length);
                return null;
            }

            byte[] payload = new byte[dataLen];
            Buffer.BlockCopy(data, dataStart, payload, 0, dataLen);
            Array.Clear(data, 0, data.Length);
            return payload;
        }

        /// <summary>
        /// Writes the new exe (program + marker + encrypted data) to the staging .tmp file.
        /// </summary>
        public static void WriteData(string exePath, byte[] encryptedData)
        {
            byte[] exe = File.ReadAllBytes(exePath);
            int pos = FindMarker(exe);
            int cleanLen = pos >= 0 ? pos : exe.Length;

            string tmpPath = GetTmpPath(exePath);
            using (var fs = new FileStream(tmpPath, FileMode.Create, FileAccess.Write))
            {
                fs.Write(exe, 0, cleanLen);
                fs.Write(Marker, 0, Marker.Length);
                fs.Write(encryptedData, 0, encryptedData.Length);
            }

            Array.Clear(exe, 0, exe.Length);
        }

        static int FindMarker(byte[] data)
        {
            for (int i = data.Length - Marker.Length; i >= 0; i--)
            {
                bool match = true;
                for (int j = 0; j < Marker.Length; j++)
                {
                    if (data[i + j] != Marker[j]) { match = false; break; }
                }
                if (match) return i;
            }
            return -1;
        }
    }
}

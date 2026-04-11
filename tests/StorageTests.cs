using System;
using System.IO;

namespace LockNote.Tests
{
    static class StorageTests
    {
        static string GetTempDir()
        {
            string dir = Path.Combine(Path.GetTempPath(), "LockNote_Tests_" + Guid.NewGuid().ToString("N").Substring(0, 8));
            Directory.CreateDirectory(dir);
            return dir;
        }

        [Test]
        public static void GetTmpPath_ReturnsDeterministicPath()
        {
            string path1 = Storage.GetTmpPath(@"C:\test\LockNote.exe");
            string path2 = Storage.GetTmpPath(@"C:\test\LockNote.exe");

            Assert.AreEqual(path1, path2, "Same exe path should produce same tmp path");
        }

        [Test]
        public static void GetTmpPath_DifferentExePaths_ProduceDifferentTmpPaths()
        {
            string path1 = Storage.GetTmpPath(@"C:\folder1\LockNote.exe");
            string path2 = Storage.GetTmpPath(@"C:\folder2\LockNote.exe");

            Assert.AreNotEqual(path1, path2, "Different exe paths should produce different tmp paths");
        }

        [Test]
        public static void GetTmpPath_CaseInsensitive()
        {
            string path1 = Storage.GetTmpPath(@"C:\Test\LockNote.exe");
            string path2 = Storage.GetTmpPath(@"C:\TEST\LOCKNOTE.EXE");

            Assert.AreEqual(path1, path2, "Paths should be case-insensitive");
        }

        [Test]
        public static void ReadData_NonExistentFile_ReturnsNull()
        {
            string result = null;
            byte[] data = Storage.ReadData(@"C:\nonexistent_path_12345\fake.exe");
            Assert.IsNull(data, "Non-existent file should return null");
        }

        [Test]
        public static void WriteData_ReadData_RoundTrip()
        {
            string tmpDir = GetTempDir();
            try
            {
                // Create a fake exe file
                string fakePath = Path.Combine(tmpDir, "test.exe");
                byte[] fakeExe = new byte[] { 0x4D, 0x5A, 0x00, 0x01, 0x02, 0x03 }; // MZ header stub
                File.WriteAllBytes(fakePath, fakeExe);

                // Encrypt some data
                byte[] encrypted = Crypto.Encrypt("test content", "password");

                // Write via Storage
                Storage.WriteData(fakePath, encrypted);

                // Read from the tmp file
                string tmpPath = Storage.GetTmpPath(fakePath);
                Assert.IsTrue(File.Exists(tmpPath), "Tmp file should exist after WriteData");

                byte[] readBack = Storage.ReadData(tmpPath);
                Assert.IsNotNull(readBack, "ReadData should find the payload");

                // Decrypt and verify
                string decrypted = Crypto.Decrypt(readBack, "password");
                Assert.AreEqual("test content", decrypted, "Round-trip through Storage should preserve content");
            }
            finally
            {
                try { Directory.Delete(tmpDir, true); } catch { }
            }
        }

        [Test]
        public static void ReadData_FileWithoutMarker_ReturnsNull()
        {
            string tmpDir = GetTempDir();
            try
            {
                string path = Path.Combine(tmpDir, "nomarker.exe");
                File.WriteAllBytes(path, new byte[] { 0x4D, 0x5A, 0x00, 0x01 });

                byte[] result = Storage.ReadData(path);
                Assert.IsNull(result, "File without marker should return null");
            }
            finally
            {
                try { Directory.Delete(tmpDir, true); } catch { }
            }
        }

        [Test]
        public static void WriteData_OverwritesPreviousPayload()
        {
            string tmpDir = GetTempDir();
            try
            {
                string fakePath = Path.Combine(tmpDir, "test.exe");
                File.WriteAllBytes(fakePath, new byte[] { 0x4D, 0x5A });

                // Write first payload
                byte[] enc1 = Crypto.Encrypt("first", "pw");
                Storage.WriteData(fakePath, enc1);
                string tmpPath = Storage.GetTmpPath(fakePath);

                // Write second payload (using the tmp as source, simulating swap)
                byte[] enc2 = Crypto.Encrypt("second", "pw");
                Storage.WriteData(tmpPath, enc2);

                // Verify second payload (re-read from tmp path)
                // The second WriteData writes to the tmp of tmpPath, so read from there
                string tmpPath2 = Storage.GetTmpPath(tmpPath);
                byte[] readBack = Storage.ReadData(tmpPath2);
                Assert.IsNotNull(readBack, "Second write should produce valid data");

                string decrypted = Crypto.Decrypt(readBack, "pw");
                Assert.AreEqual("second", decrypted, "Second payload should overwrite the first");
            }
            finally
            {
                try { Directory.Delete(tmpDir, true); } catch { }
            }
        }
    }
}

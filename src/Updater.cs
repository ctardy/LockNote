using System;
using System.ComponentModel;
using System.IO;
using System.Net;
using System.Reflection;
using System.Text;
using System.Windows.Forms;

namespace LockNote
{
    /// <summary>
    /// Checks GitHub releases for a newer version of LockNote, downloads it,
    /// and migrates encrypted data from the current exe to the new one.
    /// Uses only System.Net.WebClient (no external dependencies).
    /// </summary>
    static class Updater
    {
        public const string CurrentVersion = "1.0.0";
        const string GitHubRepo = "ctardy/LockNote";
        const string ApiUrl = "https://api.github.com/repos/" + GitHubRepo + "/releases/latest";

        /// <summary>
        /// Silent background check at startup. Only prompts the user if an update is found.
        /// </summary>
        public static void CheckOnStartup(string exePath, Form owner)
        {
            var worker = new BackgroundWorker();
            worker.DoWork += (s, e) =>
            {
                string version;
                string url;
                FetchLatestRelease(out version, out url);
                e.Result = new string[] { version, url };
            };
            worker.RunWorkerCompleted += (s, e) =>
            {
                if (e.Error != null || e.Result == null) return;

                string[] result = (string[])e.Result;
                string latestVersion = result[0];
                string downloadUrl = result[1];
                if (latestVersion == null || downloadUrl == null) return;
                if (CompareVersions(CurrentVersion, latestVersion) >= 0) return;

                PromptAndApply(exePath, latestVersion, downloadUrl, owner);
            };
            worker.RunWorkerAsync();
        }

        /// <summary>
        /// Manual check from Help menu. Shows feedback even if already up to date.
        /// </summary>
        public static void CheckForUpdate(string exePath, IWin32Window owner)
        {
            string latestVersion;
            string downloadUrl;

            try
            {
                FetchLatestRelease(out latestVersion, out downloadUrl);
            }
            catch (Exception ex)
            {
                MessageBox.Show(
                    "Unable to check for updates:\n" + ex.Message,
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            if (latestVersion == null || downloadUrl == null)
            {
                MessageBox.Show("No release found.", "LockNote",
                    MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }

            if (CompareVersions(CurrentVersion, latestVersion) >= 0)
            {
                MessageBox.Show(
                    string.Format("You are running the latest version ({0}).", CurrentVersion),
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }

            PromptAndApply(exePath, latestVersion, downloadUrl, owner);
        }

        static void PromptAndApply(string exePath, string latestVersion, string downloadUrl, IWin32Window owner)
        {
            var confirm = MessageBox.Show(
                string.Format(
                    "A new version is available: {0}\nCurrent version: {1}\n\n" +
                    "Download and install?\nYour notes will be preserved.",
                    latestVersion, CurrentVersion),
                "LockNote - Update available",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (confirm != DialogResult.Yes) return;

            try
            {
                ApplyUpdate(exePath, downloadUrl);
                MessageBox.Show(
                    "Update downloaded successfully.\nRestart LockNote to apply the new version.",
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
            catch (Exception ex)
            {
                MessageBox.Show("Update failed:\n" + ex.Message,
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        // ── GitHub API ──

        static void FetchLatestRelease(out string version, out string downloadUrl)
        {
            version = null;
            downloadUrl = null;

            using (var wc = new WebClient())
            {
                wc.Headers.Add("User-Agent", "LockNote/" + CurrentVersion);
                wc.Encoding = Encoding.UTF8;

                string json = wc.DownloadString(ApiUrl);

                version = ExtractJsonString(json, "tag_name");
                if (version != null && version.StartsWith("v"))
                    version = version.Substring(1);

                // Find the .zip asset URL
                int searchFrom = 0;
                while (true)
                {
                    string url = ExtractJsonStringFrom(json, "browser_download_url", searchFrom, out searchFrom);
                    if (url == null) break;
                    if (url.EndsWith(".zip", StringComparison.OrdinalIgnoreCase))
                    {
                        downloadUrl = url;
                        break;
                    }
                }
            }
        }

        // ── Update application ──

        static void ApplyUpdate(string exePath, string downloadUrl)
        {
            string tmpDir = Path.Combine(Path.GetTempPath(), "LockNote_update_" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(tmpDir);

            try
            {
                string zipPath = Path.Combine(tmpDir, "update.zip");

                using (var wc = new WebClient())
                {
                    wc.Headers.Add("User-Agent", "LockNote/" + CurrentVersion);
                    wc.DownloadFile(downloadUrl, zipPath);
                }

                // Extract zip using Shell32 COM (no external dependency)
                string extractDir = Path.Combine(tmpDir, "extracted");
                Directory.CreateDirectory(extractDir);
                ExtractZipViaCom(zipPath, extractDir);

                string newExePath = FindExeInDir(extractDir);
                if (newExePath == null)
                    throw new FileNotFoundException("LockNote.exe not found in the update package.");

                // Read encrypted data from current source (prefer .tmp over .exe)
                byte[] currentData = null;
                string currentTmpPath = Storage.GetTmpPath(exePath);
                if (File.Exists(currentTmpPath))
                    currentData = Storage.ReadData(currentTmpPath);
                if (currentData == null)
                    currentData = Storage.ReadData(exePath);

                // Write new exe + existing encrypted data to staging .tmp
                byte[] newExeBytes = File.ReadAllBytes(newExePath);
                string stagingPath = Storage.GetTmpPath(exePath);

                using (var fs = new FileStream(stagingPath, FileMode.Create, FileAccess.Write))
                {
                    fs.Write(newExeBytes, 0, newExeBytes.Length);

                    if (currentData != null && currentData.Length > 0)
                    {
                        byte[] marker = Storage.GetMarkerForUpdate();
                        fs.Write(marker, 0, marker.Length);
                        fs.Write(currentData, 0, currentData.Length);
                    }
                }

                Array.Clear(newExeBytes, 0, newExeBytes.Length);
            }
            finally
            {
                try { Directory.Delete(tmpDir, true); } catch { }
            }
        }

        /// <summary>
        /// Extract zip using Shell32 COM via reflection (C# 5 compatible, no dynamic).
        /// </summary>
        static void ExtractZipViaCom(string zipPath, string destDir)
        {
            Type shellType = Type.GetTypeFromProgID("Shell.Application");
            object shell = Activator.CreateInstance(shellType);
            try
            {
                // shell.NameSpace(zipPath)
                object zipFolder = shellType.InvokeMember("NameSpace",
                    BindingFlags.InvokeMethod, null, shell, new object[] { zipPath });
                // shell.NameSpace(destDir)
                object destFolder = shellType.InvokeMember("NameSpace",
                    BindingFlags.InvokeMethod, null, shell, new object[] { destDir });
                // zipFolder.Items()
                object items = zipFolder.GetType().InvokeMember("Items",
                    BindingFlags.InvokeMethod, null, zipFolder, null);
                // destFolder.CopyHere(items, 0x14)  — 0x14 = no progress + yes to all
                destFolder.GetType().InvokeMember("CopyHere",
                    BindingFlags.InvokeMethod, null, destFolder, new object[] { items, 0x14 });
            }
            finally
            {
                System.Runtime.InteropServices.Marshal.ReleaseComObject(shell);
            }
        }

        static string FindExeInDir(string dir)
        {
            foreach (string file in Directory.GetFiles(dir, "LockNote.exe", SearchOption.AllDirectories))
                return file;
            return null;
        }

        // ── Minimal JSON parsing ──

        static string ExtractJsonString(string json, string key)
        {
            int dummy;
            return ExtractJsonStringFrom(json, key, 0, out dummy);
        }

        static string ExtractJsonStringFrom(string json, string key, int startFrom, out int nextSearchPos)
        {
            string pattern = "\"" + key + "\"";
            int idx = json.IndexOf(pattern, startFrom, StringComparison.Ordinal);
            nextSearchPos = startFrom;
            if (idx < 0) return null;

            int colon = json.IndexOf(':', idx + pattern.Length);
            if (colon < 0) return null;

            int openQuote = json.IndexOf('"', colon + 1);
            if (openQuote < 0) return null;

            int closeQuote = openQuote + 1;
            while (closeQuote < json.Length)
            {
                if (json[closeQuote] == '"' && json[closeQuote - 1] != '\\')
                    break;
                closeQuote++;
            }
            if (closeQuote >= json.Length) return null;

            nextSearchPos = closeQuote + 1;
            return json.Substring(openQuote + 1, closeQuote - openQuote - 1);
        }

        // ── Version comparison ──

        static int CompareVersions(string a, string b)
        {
            string[] pa = a.Split('.');
            string[] pb = b.Split('.');
            int len = Math.Max(pa.Length, pb.Length);
            for (int i = 0; i < len; i++)
            {
                int va = i < pa.Length ? ParseInt(pa[i]) : 0;
                int vb = i < pb.Length ? ParseInt(pb[i]) : 0;
                if (va != vb) return va - vb;
            }
            return 0;
        }

        static int ParseInt(string s)
        {
            int result = 0;
            for (int i = 0; i < s.Length; i++)
            {
                char c = s[i];
                if (c >= '0' && c <= '9')
                    result = result * 10 + (c - '0');
            }
            return result;
        }
    }
}

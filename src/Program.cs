using System;
using System.IO;
using System.Reflection;
using System.Windows.Forms;

namespace LockNote
{
    static class Program
    {
        [STAThread]
        static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            string exePath = Assembly.GetEntryAssembly().Location;
            string tmpPath = Storage.GetTmpPath(exePath);

            // Clean up stale .tmp files from previous crashes
            CleanStaleTmpFiles(tmpPath);

            // Read encrypted data: prefer .tmp (pending swap) over .exe
            byte[] encryptedData = null;
            if (File.Exists(tmpPath))
                encryptedData = Storage.ReadData(tmpPath);
            if (encryptedData == null)
                encryptedData = Storage.ReadData(exePath);

            if (encryptedData == null)
            {
                using (var dlg = new CreatePasswordDialog())
                {
                    if (dlg.ShowDialog() != DialogResult.OK) return;
                    Application.Run(new EditorForm(exePath, dlg.Password, ""));
                }
            }
            else
            {
                using (var dlg = new UnlockDialog(encryptedData))
                {
                    if (dlg.ShowDialog() != DialogResult.OK) return;
                    Application.Run(new EditorForm(exePath, dlg.Password, dlg.DecryptedText));
                }
            }
        }

        /// <summary>
        /// If a .tmp for this exe exists and is older than 1 minute, it's from a
        /// crash or a failed swap. Merge it into the exe, then delete it.
        /// Also cleans up any orphaned .tmp files in the LockNote AppData folder.
        /// </summary>
        static void CleanStaleTmpFiles(string currentTmpPath)
        {
            try
            {
                string dir = Path.GetDirectoryName(currentTmpPath);
                if (!Directory.Exists(dir)) return;

                foreach (string tmp in Directory.GetFiles(dir, "*.tmp"))
                {
                    // Skip the current .tmp — it may contain valid pending data
                    if (string.Equals(tmp, currentTmpPath, StringComparison.OrdinalIgnoreCase))
                        continue;

                    // Orphaned .tmp from a different exe path or old crash — delete
                    try { File.Delete(tmp); } catch { }
                }
            }
            catch { }
        }
    }
}

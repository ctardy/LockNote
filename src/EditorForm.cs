using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Windows.Forms;

namespace LockNote
{
    class EditorForm : Form
    {
        LineNumberTextBox txtEditor;
        SearchBar searchBar;
        string exePath;
        string password;
        string savedText;
        bool modified;
        Settings settings;
        bool hasPendingTmp;

        public EditorForm(string path, string pwd, string rawText)
        {
            exePath = path;
            password = pwd;

            string noteText;
            settings = Settings.ParseFrom(rawText, out noteText);
            savedText = noteText;

            Text = "LockNote";
            ClientSize = new Size(700, 500);
            StartPosition = FormStartPosition.CenterScreen;
            Icon = SystemIcons.Shield;

            // ── Menu ──
            var menuStrip = new MenuStrip();

            var menuFile = new ToolStripMenuItem("File");
            menuFile.DropDownItems.Add("Save\tCtrl+S", null, (s, e) => Save());
            menuFile.DropDownItems.Add("Change password", null, (s, e) => ChangePassword());
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Settings", null, (s, e) => OpenSettings());
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Quit\tCtrl+Q", null, (s, e) => Close());

            var menuEdit = new ToolStripMenuItem("Edit");
            menuEdit.DropDownItems.Add("Find\tCtrl+F", null, (s, e) => searchBar.ShowAndFocus());
            menuEdit.DropDownItems.Add("Select all\tCtrl+A", null, (s, e) => txtEditor.SelectAll());
            menuEdit.DropDownItems.Add("Insert timestamp\tF5", null, (s, e) => InsertTimestamp());

            menuStrip.Items.AddRange(new ToolStripItem[] { menuFile, menuEdit });
            MainMenuStrip = menuStrip;

            // ── Editor ──
            txtEditor = new LineNumberTextBox
            {
                Dock = DockStyle.Fill,
                EditorFont = new Font("Consolas", 11f),
                TextForeColor = Color.Black,
                TextBackColor = Color.White,
                ContentText = noteText
            };
            txtEditor.ContentChanged += (s, e) => UpdateTitle();

            // ── Search bar ──
            searchBar = new SearchBar(txtEditor);

            Controls.Add(txtEditor);
            Controls.Add(searchBar);
            Controls.Add(menuStrip);

            KeyPreview = true;
            KeyDown += OnKeyDown;
            FormClosing += OnFormClosing;
        }

        void UpdateTitle()
        {
            modified = (txtEditor.ContentText != savedText);
            Text = modified ? "LockNote *" : "LockNote";
        }

        void OnKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Control)
            {
                switch (e.KeyCode)
                {
                    case Keys.S: Save(); e.SuppressKeyPress = true; break;
                    case Keys.F: searchBar.ShowAndFocus(); e.SuppressKeyPress = true; break;
                    case Keys.Q: Close(); e.SuppressKeyPress = true; break;
                }
            }

            if (e.KeyCode == Keys.F5)
            {
                InsertTimestamp();
                e.SuppressKeyPress = true;
            }
        }

        void InsertTimestamp()
        {
            txtEditor.InsertAtCursor(DateTime.Now.ToString("yyyy-MM-dd HH:mm"));
        }

        void Save()
        {
            try
            {
                string payload = settings.PrependTo(txtEditor.ContentText);
                byte[] encrypted = Crypto.Encrypt(payload, password);
                Storage.WriteData(exePath, encrypted);
                Array.Clear(encrypted, 0, encrypted.Length);

                savedText = txtEditor.ContentText;
                modified = false;
                hasPendingTmp = true;
                Text = "LockNote";
            }
            catch (Exception ex)
            {
                MessageBox.Show("Save failed:\n" + ex.Message,
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        void ChangePassword()
        {
            using (var dlg = new CreatePasswordDialog())
            {
                dlg.Text = "LockNote - Change password";
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    password = dlg.Password;
                    Save();
                }
            }
        }

        void OpenSettings()
        {
            using (var dlg = new SettingsDialog(settings))
            {
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    settings = dlg.Result;
                    Save();
                }
            }
        }

        void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            // Recheck in case the event didn't propagate
            modified = (txtEditor.ContentText != savedText);

            if (modified)
            {
                switch (settings.SaveOnClose)
                {
                    case CloseAction.Always:
                        Save();
                        break;

                    case CloseAction.Never:
                        break;

                    case CloseAction.Ask:
                    default:
                        using (var dlg = new CloseConfirmDialog())
                        {
                            var r = dlg.ShowDialog(this);
                            if (r == DialogResult.Yes)
                            {
                                if (dlg.RememberChoice)
                                    settings.SaveOnClose = CloseAction.Always;
                                Save();
                            }
                            else if (r == DialogResult.No)
                            {
                                if (dlg.RememberChoice)
                                {
                                    settings.SaveOnClose = CloseAction.Never;
                                    Save();
                                }
                            }
                            else
                            {
                                e.Cancel = true;
                                return;
                            }
                        }
                        break;
                }
            }

            txtEditor.Clear();
            password = null;
            savedText = null;

            // Spawn a hidden cmd that waits for the exe lock to be released,
            // then moves .tmp over .exe — result: always a single file.
            if (hasPendingTmp)
            {
                string tmpPath = Storage.GetTmpPath(exePath);
                if (File.Exists(tmpPath))
                {
                    var psi = new ProcessStartInfo();
                    psi.FileName = "cmd.exe";
                    // ping is more reliable than timeout in a hidden window
                    psi.Arguments = string.Format(
                        "/c ping -n 3 127.0.0.1 >nul & move /y \"{0}\" \"{1}\"",
                        tmpPath, exePath);
                    psi.WindowStyle = ProcessWindowStyle.Hidden;
                    psi.CreateNoWindow = true;
                    psi.UseShellExecute = false;
                    Process.Start(psi);
                }
            }
        }
    }
}

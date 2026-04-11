using System;
using System.ComponentModel;
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
        StatusStrip statusStrip;
        ToolStripStatusLabel lblStats;
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

            Theme.SetMode(settings.ThemeMode);

            Text = "LockNote";
            ClientSize = new Size(700, 500);
            StartPosition = FormStartPosition.CenterScreen;
            Icon = SystemIcons.Shield;
            BackColor = Theme.Background;

            // ── Menu ──
            var menuStrip = new MenuStrip();

            var menuFile = new ToolStripMenuItem("File");
            menuFile.DropDownItems.Add("Save\tCtrl+S", null, (s, e) => Save());
            menuFile.DropDownItems.Add("Change password", null, (s, e) => ChangePassword());
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Settings", null, (s, e) => OpenSettings());
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Quit\tCtrl+Q", null, (s, e) => Close());

            var menuView = new ToolStripMenuItem("View");
            var menuAlwaysOnTop = new ToolStripMenuItem("Always on top");
            menuAlwaysOnTop.CheckOnClick = true;
            menuAlwaysOnTop.Click += (s, e) => { TopMost = menuAlwaysOnTop.Checked; };
            menuView.DropDownItems.Add(menuAlwaysOnTop);

            var menuEdit = new ToolStripMenuItem("Edit");
            menuEdit.DropDownItems.Add("Cut\tCtrl+X", null, (s, e) => txtEditor.Cut());
            menuEdit.DropDownItems.Add("Copy\tCtrl+C", null, (s, e) => txtEditor.Copy());
            menuEdit.DropDownItems.Add("Paste\tCtrl+V", null, (s, e) => txtEditor.Paste());
            menuEdit.DropDownItems.Add("Paste plain text\tCtrl+Shift+V", null, (s, e) => txtEditor.PastePlainText());
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Find\tCtrl+F", null, (s, e) => searchBar.ShowAndFocus());
            menuEdit.DropDownItems.Add("Go to line\tCtrl+G", null, (s, e) => GoToLine());
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Duplicate line\tCtrl+D", null, (s, e) => txtEditor.DuplicateLine());
            menuEdit.DropDownItems.Add("Delete line\tCtrl+Shift+K", null, (s, e) => txtEditor.DeleteLine());
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Select all\tCtrl+A", null, (s, e) => txtEditor.SelectAll());
            menuEdit.DropDownItems.Add("Insert timestamp\tF5", null, (s, e) => InsertTimestamp());

            menuStrip.Items.AddRange(new ToolStripItem[] { menuFile, menuView, menuEdit });
            MainMenuStrip = menuStrip;
            Theme.ApplyToMenuStrip(menuStrip);

            // ── Editor ──
            txtEditor = new LineNumberTextBox
            {
                Dock = DockStyle.Fill,
                EditorFont = Theme.EditorFont,
                TextForeColor = Theme.EditorText,
                TextBackColor = Theme.EditorBackground,
                GutterBackColor = Theme.GutterBackground,
                GutterForeColor = Theme.GutterText,
                ContentText = noteText
            };
            txtEditor.ContentChanged += (s, e) =>
            {
                if (!modified)
                {
                    modified = true;
                    Text = "LockNote *";
                }
                UpdateStatusBar();
            };

            // ── Search bar ──
            searchBar = new SearchBar(txtEditor);

            // ── Status bar ──
            statusStrip = new StatusStrip();
            lblStats = new ToolStripStatusLabel();
            lblStats.Spring = true;
            lblStats.TextAlign = System.Drawing.ContentAlignment.MiddleRight;
            statusStrip.Items.Add(lblStats);
            Theme.ApplyToStatusStrip(statusStrip);

            Controls.Add(txtEditor);
            Controls.Add(searchBar);
            Controls.Add(statusStrip);
            Controls.Add(menuStrip);

            KeyPreview = true;
            KeyDown += OnKeyDown;
            FormClosing += OnFormClosing;

            UpdateStatusBar();
        }

        void UpdateStatusBar()
        {
            string text = txtEditor.ContentText;
            int chars = text.Length;
            int words = 0;
            int lines = 1;
            bool inWord = false;
            for (int i = 0; i < chars; i++)
            {
                char c = text[i];
                if (c == '\n')
                {
                    lines++;
                    inWord = false;
                }
                else if (char.IsWhiteSpace(c))
                {
                    inWord = false;
                }
                else if (!inWord)
                {
                    inWord = true;
                    words++;
                }
            }
            lblStats.Text = string.Format("{0} words  |  {1} chars  |  {2} lines", words, chars, lines);
        }

        void MarkClean()
        {
            modified = false;
            savedText = txtEditor.ContentText;
            Text = "LockNote";
        }

        void OnKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Control && e.Shift)
            {
                switch (e.KeyCode)
                {
                    case Keys.V: txtEditor.PastePlainText(); e.SuppressKeyPress = true; return;
                    case Keys.K: txtEditor.DeleteLine(); e.SuppressKeyPress = true; return;
                }
            }

            if (e.Control && !e.Shift)
            {
                switch (e.KeyCode)
                {
                    case Keys.S: Save(); e.SuppressKeyPress = true; break;
                    case Keys.F: searchBar.ShowAndFocus(); e.SuppressKeyPress = true; break;
                    case Keys.G: GoToLine(); e.SuppressKeyPress = true; break;
                    case Keys.D: txtEditor.DuplicateLine(); e.SuppressKeyPress = true; break;
                    case Keys.Q: Close(); e.SuppressKeyPress = true; break;
                }
            }

            if (e.KeyCode == Keys.F5 && !e.Control && !e.Alt)
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
                Cursor = Cursors.WaitCursor;
                string payload = settings.PrependTo(txtEditor.ContentText);
                byte[] encrypted = Crypto.Encrypt(payload, password);
                Storage.WriteData(exePath, encrypted);
                Array.Clear(encrypted, 0, encrypted.Length);

                hasPendingTmp = true;
                MarkClean();
            }
            catch (Exception ex)
            {
                MessageBox.Show("Save failed:\n" + ex.Message,
                    "LockNote", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            finally
            {
                Cursor = Cursors.Default;
            }
        }

        void GoToLine()
        {
            int current = txtEditor.GetCurrentLineNumber();
            int total = txtEditor.GetTotalLines();
            using (var dlg = new GoToLineDialog(current, total))
            {
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    txtEditor.GoToLine(dlg.LineNumber);
                }
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
                    bool themeChanged = (settings.ThemeMode != dlg.Result.ThemeMode);
                    settings = dlg.Result;
                    if (themeChanged)
                    {
                        Theme.SetMode(settings.ThemeMode);
                        ApplyTheme();
                    }
                    Save();
                }
            }
        }

        void ApplyTheme()
        {
            BackColor = Theme.Background;
            txtEditor.TextForeColor = Theme.EditorText;
            txtEditor.TextBackColor = Theme.EditorBackground;
            txtEditor.GutterBackColor = Theme.GutterBackground;
            txtEditor.GutterForeColor = Theme.GutterText;
            searchBar.BackColor = Theme.Surface;
            Theme.ApplyToControls(searchBar.Controls);
            Theme.ApplyToMenuStrip(MainMenuStrip as MenuStrip);
            Theme.ApplyToStatusStrip(statusStrip);
        }

        void OnFormClosing(object sender, FormClosingEventArgs e)
        {
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

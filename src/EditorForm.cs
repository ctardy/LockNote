using System;
using System.Collections.Generic;
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
        TabBar tabBar;
        StatusStrip statusStrip;
        ToolStripStatusLabel lblStats;
        string exePath;
        string password;
        List<NoteTab> tabs;
        int activeTabIndex;
        Settings settings;
        bool hasPendingTmp;

        public EditorForm(string path, string pwd, string rawText)
        {
            exePath = path;
            password = pwd;

            string noteText;
            settings = Settings.ParseFrom(rawText, out noteText);

            tabs = TabStore.Parse(noteText);
            activeTabIndex = Math.Min(settings.ActiveTab, tabs.Count - 1);
            if (activeTabIndex < 0) activeTabIndex = 0;

            Theme.SetMode(settings.ThemeMode);

            Text = "LockNote";
            ClientSize = new Size(700, 500);
            StartPosition = FormStartPosition.CenterScreen;
            Icon = SystemIcons.Shield;
            BackColor = Theme.Background;

            // -- Menu --
            var menuStrip = new MenuStrip();

            var menuFile = new ToolStripMenuItem("File");
            menuFile.DropDownItems.Add("Save\tCtrl+S", null, delegate { Save(); });
            menuFile.DropDownItems.Add("Change password", null, delegate { ChangePassword(); });
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Settings", null, delegate { OpenSettings(); });
            menuFile.DropDownItems.Add(new ToolStripSeparator());
            menuFile.DropDownItems.Add("Quit\tCtrl+Q", null, delegate { Close(); });

            var menuView = new ToolStripMenuItem("View");
            var menuAlwaysOnTop = new ToolStripMenuItem("Always on top");
            menuAlwaysOnTop.CheckOnClick = true;
            menuAlwaysOnTop.Click += delegate { TopMost = menuAlwaysOnTop.Checked; };
            menuView.DropDownItems.Add(menuAlwaysOnTop);

            var menuTab = new ToolStripMenuItem("Tab");
            menuTab.DropDownItems.Add("New tab\tCtrl+T", null, delegate { AddNewTab(); });
            menuTab.DropDownItems.Add("Close tab\tCtrl+W", null, delegate { CloseTab(activeTabIndex); });
            menuTab.DropDownItems.Add("Rename tab", null, delegate { RenameTab(activeTabIndex); });
            menuTab.DropDownItems.Add(new ToolStripSeparator());
            menuTab.DropDownItems.Add("Next tab\tCtrl+Tab", null, delegate { SwitchToTab(activeTabIndex < tabs.Count - 1 ? activeTabIndex + 1 : 0); });
            menuTab.DropDownItems.Add("Previous tab\tCtrl+Shift+Tab", null, delegate { SwitchToTab(activeTabIndex > 0 ? activeTabIndex - 1 : tabs.Count - 1); });

            var menuEdit = new ToolStripMenuItem("Edit");
            menuEdit.DropDownItems.Add("Cut\tCtrl+X", null, delegate { txtEditor.Cut(); });
            menuEdit.DropDownItems.Add("Copy\tCtrl+C", null, delegate { txtEditor.Copy(); });
            menuEdit.DropDownItems.Add("Paste\tCtrl+V", null, delegate { txtEditor.Paste(); });
            menuEdit.DropDownItems.Add("Paste plain text\tCtrl+Shift+V", null, delegate { txtEditor.PastePlainText(); });
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Find\tCtrl+F", null, delegate { searchBar.ShowAndFocus(); });
            menuEdit.DropDownItems.Add("Go to line\tCtrl+G", null, delegate { GoToLine(); });
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Duplicate line\tCtrl+D", null, delegate { txtEditor.DuplicateLine(); });
            menuEdit.DropDownItems.Add("Delete line\tCtrl+Shift+K", null, delegate { txtEditor.DeleteLine(); });
            menuEdit.DropDownItems.Add(new ToolStripSeparator());
            menuEdit.DropDownItems.Add("Select all\tCtrl+A", null, delegate { txtEditor.SelectAll(); });
            menuEdit.DropDownItems.Add("Insert timestamp\tF5", null, delegate { InsertTimestamp(); });

            var menuHelp = new ToolStripMenuItem("Help");
            menuHelp.DropDownItems.Add("Check for updates", null, delegate { Updater.CheckForUpdate(exePath, this); });
            menuHelp.DropDownItems.Add(new ToolStripSeparator());
            menuHelp.DropDownItems.Add(string.Format("About LockNote v{0}", Updater.CurrentVersion));

            menuStrip.Items.AddRange(new ToolStripItem[] { menuFile, menuView, menuTab, menuEdit, menuHelp });
            MainMenuStrip = menuStrip;
            Theme.ApplyToMenuStrip(menuStrip);

            // -- Editor --
            txtEditor = new LineNumberTextBox
            {
                Dock = DockStyle.Fill,
                EditorFont = Theme.EditorFont,
                TextForeColor = Theme.EditorText,
                TextBackColor = Theme.EditorBackground,
                GutterBackColor = Theme.GutterBackground,
                GutterForeColor = Theme.GutterText,
                ContentText = tabs[activeTabIndex].Content
            };
            txtEditor.ContentChanged += delegate
            {
                tabs[activeTabIndex].Content = txtEditor.ContentText;
                UpdateTitle();
                UpdateTabBar();
                UpdateStatusBar();
            };

            // -- Search bar --
            searchBar = new SearchBar(txtEditor);

            // -- Tab bar --
            tabBar = new TabBar();
            tabBar.ActiveTabChanged += delegate(object s, TabEventArgs ev) { SwitchToTab(ev.TabIndex); };
            tabBar.TabCloseRequested += delegate(object s, TabEventArgs ev) { CloseTab(ev.TabIndex); };
            tabBar.TabRenameRequested += delegate(object s, TabEventArgs ev) { RenameTab(ev.TabIndex); };
            tabBar.NewTabRequested += delegate { AddNewTab(); };

            // -- Status bar --
            statusStrip = new StatusStrip();
            lblStats = new ToolStripStatusLabel();
            lblStats.Spring = true;
            lblStats.TextAlign = System.Drawing.ContentAlignment.MiddleRight;
            statusStrip.Items.Add(lblStats);
            Theme.ApplyToStatusStrip(statusStrip);

            Controls.Add(txtEditor);
            Controls.Add(searchBar);
            Controls.Add(tabBar);
            Controls.Add(statusStrip);
            Controls.Add(menuStrip);

            KeyPreview = true;
            KeyDown += OnKeyDown;
            FormClosing += OnFormClosing;
            Shown += delegate { Updater.CheckOnStartup(exePath, this); };

            UpdateTabBar();
            UpdateStatusBar();
        }

        void UpdateTabBar()
        {
            var names = new List<string>();
            var modified = new List<bool>();
            for (int i = 0; i < tabs.Count; i++)
            {
                names.Add(tabs[i].Name);
                modified.Add(tabs[i].Modified);
            }
            tabBar.SetTabs(names, modified, activeTabIndex);
        }

        void UpdateTitle()
        {
            bool anyModified = false;
            for (int i = 0; i < tabs.Count; i++)
            {
                if (tabs[i].Modified) { anyModified = true; break; }
            }
            Text = anyModified ? "LockNote *" : "LockNote";
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

        void SwitchToTab(int index)
        {
            if (index < 0 || index >= tabs.Count || index == activeTabIndex) return;
            tabs[activeTabIndex].Content = txtEditor.ContentText;
            activeTabIndex = index;
            txtEditor.ContentText = tabs[activeTabIndex].Content;
            UpdateTabBar();
            UpdateStatusBar();
            UpdateTitle();
        }

        void AddNewTab()
        {
            int num = TabStore.NextTabNumber(tabs);
            tabs.Add(new NoteTab("Note " + num, ""));
            SwitchToTab(tabs.Count - 1);
        }

        void CloseTab(int index)
        {
            if (tabs.Count <= 1) return;
            tabs.RemoveAt(index);
            if (activeTabIndex >= tabs.Count) activeTabIndex = tabs.Count - 1;
            else if (activeTabIndex > index) activeTabIndex--;
            else if (activeTabIndex == index)
            {
                if (activeTabIndex >= tabs.Count) activeTabIndex = tabs.Count - 1;
            }
            txtEditor.ContentText = tabs[activeTabIndex].Content;
            UpdateTabBar();
            UpdateTitle();
            UpdateStatusBar();
        }

        void RenameTab(int index)
        {
            if (index < 0 || index >= tabs.Count) return;
            using (var dlg = new RenameTabDialog(tabs[index].Name))
            {
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    tabs[index].Name = dlg.TabName;
                    UpdateTabBar();
                }
            }
        }

        void OnKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Control && e.Shift)
            {
                switch (e.KeyCode)
                {
                    case Keys.V: txtEditor.PastePlainText(); e.SuppressKeyPress = true; return;
                    case Keys.K: txtEditor.DeleteLine(); e.SuppressKeyPress = true; return;
                    case Keys.Tab: SwitchToTab(activeTabIndex > 0 ? activeTabIndex - 1 : tabs.Count - 1); e.SuppressKeyPress = true; return;
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
                    case Keys.T: AddNewTab(); e.SuppressKeyPress = true; break;
                    case Keys.W: CloseTab(activeTabIndex); e.SuppressKeyPress = true; break;
                    case Keys.Tab: SwitchToTab(activeTabIndex < tabs.Count - 1 ? activeTabIndex + 1 : 0); e.SuppressKeyPress = true; break;
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
                tabs[activeTabIndex].Content = txtEditor.ContentText;
                settings.ActiveTab = activeTabIndex;
                string tabData = TabStore.Serialize(tabs);
                string payload = settings.PrependTo(tabData);
                byte[] encrypted = Crypto.Encrypt(payload, password);
                Storage.WriteData(exePath, encrypted);
                Array.Clear(encrypted, 0, encrypted.Length);

                for (int i = 0; i < tabs.Count; i++)
                    tabs[i].MarkClean();
                hasPendingTmp = true;
                UpdateTitle();
                UpdateTabBar();
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
            tabBar.BackColor = Theme.Background;
            tabBar.Invalidate();
        }

        void OnFormClosing(object sender, FormClosingEventArgs e)
        {
            bool anyModified = false;
            for (int i = 0; i < tabs.Count; i++)
            {
                if (tabs[i].Modified) { anyModified = true; break; }
            }

            if (anyModified)
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

            if (hasPendingTmp)
            {
                string tmpPath = Storage.GetTmpPath(exePath);
                if (File.Exists(tmpPath))
                {
                    var psi = new ProcessStartInfo();
                    psi.FileName = "cmd.exe";
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

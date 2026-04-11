using System;
using System.Collections.Generic;
using System.Text;

namespace LockNote
{
    /// <summary>
    /// User preferences stored alongside the note text inside the encrypted payload.
    /// Serialized as a header block before the note content:
    ///   [LOCKNOTE_SETTINGS]\nkey=value\nkey=value\n[/LOCKNOTE_SETTINGS]\n
    /// If no header is present, defaults are used (backward compatible with existing data).
    /// </summary>
    class Settings
    {
        const string HeaderStart = "[LOCKNOTE_SETTINGS]";
        const string HeaderEnd = "[/LOCKNOTE_SETTINGS]";

        /// <summary>
        /// What to do when closing with unsaved changes:
        ///   Ask    = show the Yes/No/Cancel dialog (default)
        ///   Always = auto-save without asking
        ///   Never  = discard without asking
        /// </summary>
        public CloseAction SaveOnClose { get; set; }
        public AppTheme ThemeMode { get; set; }
        public int ActiveTab { get; set; }

        public Settings()
        {
            SaveOnClose = CloseAction.Ask;
            ThemeMode = AppTheme.Dark;
            ActiveTab = 0;
        }

        /// <summary>
        /// Separates settings header from note text.
        /// Returns the Settings and the clean note content.
        /// </summary>
        public static Settings ParseFrom(string raw, out string noteText)
        {
            var s = new Settings();

            if (raw == null || !raw.StartsWith(HeaderStart))
            {
                noteText = raw ?? "";
                return s;
            }

            int endIdx = raw.IndexOf(HeaderEnd);
            if (endIdx < 0)
            {
                noteText = raw;
                return s;
            }

            string headerBlock = raw.Substring(HeaderStart.Length, endIdx - HeaderStart.Length);
            noteText = raw.Substring(endIdx + HeaderEnd.Length);
            if (noteText.StartsWith("\r\n")) noteText = noteText.Substring(2);
            else if (noteText.Length > 0 && (noteText[0] == '\r' || noteText[0] == '\n')) noteText = noteText.Substring(1);

            string[] lines = headerBlock.Split('\n');
            foreach (string line in lines)
            {
                string trimmed = line.Trim();
                if (trimmed.Length == 0) continue;
                int eq = trimmed.IndexOf('=');
                if (eq < 0) continue;

                string key = trimmed.Substring(0, eq).Trim();
                string val = trimmed.Substring(eq + 1).Trim();

                if (key == "save_on_close")
                {
                    if (val == "always") s.SaveOnClose = CloseAction.Always;
                    else if (val == "never") s.SaveOnClose = CloseAction.Never;
                    else s.SaveOnClose = CloseAction.Ask;
                }
                else if (key == "theme")
                {
                    if (val == "light") s.ThemeMode = AppTheme.Light;
                    else s.ThemeMode = AppTheme.Dark;
                }
                else if (key == "active_tab")
                {
                    int tabIdx;
                    if (int.TryParse(val, out tabIdx))
                        s.ActiveTab = tabIdx;
                }
            }

            return s;
        }

        /// <summary>
        /// Prepends the settings header to the note text for storage.
        /// </summary>
        public string PrependTo(string noteText)
        {
            var sb = new StringBuilder();
            sb.AppendLine(HeaderStart);

            string val = "ask";
            if (SaveOnClose == CloseAction.Always) val = "always";
            else if (SaveOnClose == CloseAction.Never) val = "never";
            sb.AppendLine("save_on_close=" + val);
            sb.AppendLine("theme=" + (ThemeMode == AppTheme.Light ? "light" : "dark"));
            sb.AppendLine("active_tab=" + ActiveTab);

            sb.AppendLine(HeaderEnd);
            sb.Append(noteText);
            return sb.ToString();
        }
    }

    enum CloseAction
    {
        Ask,
        Always,
        Never
    }

    enum AppTheme
    {
        Dark,
        Light
    }
}

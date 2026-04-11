using System;
using System.Collections.Generic;
using System.Text;

namespace LockNote
{
    static class TabStore
    {
        const string TabsStart = "[LOCKNOTE_TABS]";
        const string TabsEnd = "[/LOCKNOTE_TABS]";
        const string TabPrefix = "[TAB:";
        const string TabSuffix = "]";
        const char EscapeChar = '\x00';

        public static List<NoteTab> Parse(string text)
        {
            var tabs = new List<NoteTab>();
            if (text == null || !text.StartsWith(TabsStart))
            {
                tabs.Add(new NoteTab("Note 1", text ?? ""));
                return tabs;
            }
            int endIdx = text.LastIndexOf(TabsEnd);
            if (endIdx < 0) { tabs.Add(new NoteTab("Note 1", text)); return tabs; }

            string body = text.Substring(TabsStart.Length, endIdx - TabsStart.Length);
            if (body.StartsWith("\r\n")) body = body.Substring(2);
            else if (body.Length > 0 && (body[0] == '\r' || body[0] == '\n')) body = body.Substring(1);

            string currentName = null;
            var contentBuilder = new StringBuilder();
            string[] lines = body.Split('\n');
            for (int i = 0; i < lines.Length; i++)
            {
                string line = lines[i];
                if (line.Length > 0 && line[line.Length - 1] == '\r')
                    line = line.Substring(0, line.Length - 1);
                if (line.StartsWith(TabPrefix) && line.EndsWith(TabSuffix))
                {
                    if (currentName != null)
                    {
                        string content = contentBuilder.ToString();
                        if (content.EndsWith("\r\n")) content = content.Substring(0, content.Length - 2);
                        else if (content.Length > 0 && (content[content.Length - 1] == '\n' || content[content.Length - 1] == '\r'))
                            content = content.Substring(0, content.Length - 1);
                        tabs.Add(new NoteTab(currentName, Unescape(content)));
                    }
                    currentName = line.Substring(TabPrefix.Length, line.Length - TabPrefix.Length - TabSuffix.Length);
                    contentBuilder = new StringBuilder();
                }
                else if (currentName != null)
                {
                    if (contentBuilder.Length > 0) contentBuilder.Append('\n');
                    contentBuilder.Append(line);
                }
            }
            if (currentName != null)
            {
                string content = contentBuilder.ToString();
                if (content.EndsWith("\r\n")) content = content.Substring(0, content.Length - 2);
                else if (content.Length > 0 && (content[content.Length - 1] == '\n' || content[content.Length - 1] == '\r'))
                    content = content.Substring(0, content.Length - 1);
                tabs.Add(new NoteTab(currentName, Unescape(content)));
            }
            if (tabs.Count == 0) tabs.Add(new NoteTab("Note 1", ""));
            return tabs;
        }

        public static string Serialize(List<NoteTab> tabs)
        {
            var sb = new StringBuilder();
            sb.AppendLine(TabsStart);
            for (int i = 0; i < tabs.Count; i++)
            {
                sb.AppendLine(TabPrefix + tabs[i].Name + TabSuffix);
                sb.AppendLine(Escape(tabs[i].Content));
            }
            sb.Append(TabsEnd);
            return sb.ToString();
        }

        static string Escape(string content)
        {
            if (string.IsNullOrEmpty(content)) return content;
            var sb = new StringBuilder();
            string[] lines = content.Split('\n');
            for (int i = 0; i < lines.Length; i++)
            {
                string line = lines[i];
                if (line.StartsWith(TabPrefix) || line.StartsWith(TabsStart) || line.StartsWith(TabsEnd))
                    sb.Append(EscapeChar);
                sb.Append(line);
                if (i < lines.Length - 1) sb.Append('\n');
            }
            return sb.ToString();
        }

        static string Unescape(string content)
        {
            if (string.IsNullOrEmpty(content)) return content;
            var sb = new StringBuilder(content.Length);
            for (int i = 0; i < content.Length; i++)
            {
                if (content[i] == EscapeChar) continue;
                sb.Append(content[i]);
            }
            return sb.ToString();
        }

        public static int NextTabNumber(List<NoteTab> tabs)
        {
            int max = 0;
            for (int i = 0; i < tabs.Count; i++)
            {
                string name = tabs[i].Name;
                if (name.StartsWith("Note "))
                {
                    int num;
                    if (int.TryParse(name.Substring(5), out num) && num > max)
                        max = num;
                }
            }
            return max + 1;
        }
    }
}

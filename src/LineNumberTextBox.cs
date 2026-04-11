using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    /// <summary>Panel with double buffering to prevent flicker.</summary>
    class BufferedPanel : Panel
    {
        public BufferedPanel()
        {
            SetStyle(
                ControlStyles.AllPaintingInWmPaint |
                ControlStyles.OptimizedDoubleBuffer |
                ControlStyles.UserPaint,
                true);
        }
    }

    /// <summary>
    /// A RichTextBox with a line number gutter on the left.
    /// </summary>
    class LineNumberTextBox : UserControl
    {
        BufferedPanel gutter;
        RichTextBox rtb;
        Timer highlightTimer;
        bool suppressEvents;
        string lastHighlightedWord;
        List<int> highlightPositions = new List<int>();

        const int GutterPadding = 6;
        const int MinHighlightLength = 2;

        public event EventHandler ContentChanged;

        public LineNumberTextBox()
        {
            rtb = new RichTextBox
            {
                Dock = DockStyle.Fill,
                BorderStyle = BorderStyle.None,
                WordWrap = false,
                AcceptsTab = true,
                ScrollBars = RichTextBoxScrollBars.Both,
                DetectUrls = true,
                MaxLength = 0,
                ShortcutsEnabled = true,
                AllowDrop = true,
                EnableAutoDragDrop = false
            };

            gutter = new BufferedPanel
            {
                Dock = DockStyle.Left,
                Width = 50,
                BackColor = Color.FromArgb(240, 240, 240)
            };

            rtb.TextChanged += (s, e) =>
            {
                gutter.Invalidate();
                if (!suppressEvents)
                {
                    // Text changed by user — clear stale highlights
                    highlightPositions.Clear();
                    lastHighlightedWord = null;
                    if (ContentChanged != null) ContentChanged(this, EventArgs.Empty);
                }
            };
            rtb.VScroll += (s, e) => gutter.Invalidate();
            rtb.Resize += (s, e) => gutter.Invalidate();

            // URL click — open in default browser
            rtb.LinkClicked += (s, e) =>
            {
                try { Process.Start(e.LinkText); } catch { }
            };

            // Drag & drop text files
            rtb.DragEnter += (s, e) =>
            {
                if (e.Data.GetDataPresent(DataFormats.FileDrop) || e.Data.GetDataPresent(DataFormats.Text))
                    e.Effect = DragDropEffects.Copy;
            };
            rtb.DragDrop += OnDragDrop;

            // Context menu
            var ctx = new ContextMenuStrip();
            ctx.Items.Add("Cut\tCtrl+X", null, (s, e) => Cut());
            ctx.Items.Add("Copy\tCtrl+C", null, (s, e) => Copy());
            ctx.Items.Add("Paste\tCtrl+V", null, (s, e) => Paste());
            ctx.Items.Add("Paste plain text\tCtrl+Shift+V", null, (s, e) => PastePlainText());
            ctx.Items.Add(new ToolStripSeparator());
            ctx.Items.Add("Select all\tCtrl+A", null, (s, e) => SelectAll());
            ctx.Opening += (s, e) =>
            {
                bool hasSelection = rtb.SelectionLength > 0;
                bool hasClipboard = Clipboard.ContainsText();
                ctx.Items[0].Enabled = hasSelection; // Cut
                ctx.Items[1].Enabled = hasSelection; // Copy
                ctx.Items[2].Enabled = hasClipboard; // Paste
                ctx.Items[3].Enabled = hasClipboard; // Paste plain
            };
            rtb.ContextMenuStrip = ctx;
            Theme.ApplyToContextMenu(ctx);

            gutter.Paint += OnGutterPaint;

            highlightTimer = new Timer();
            highlightTimer.Interval = 200;
            highlightTimer.Tick += delegate { highlightTimer.Stop(); ApplyOccurrenceHighlights(); };
            rtb.SelectionChanged += delegate { OnSelectionChanged(); };

            Controls.Add(rtb);
            Controls.Add(gutter);
        }

        // ── Public properties (no override to avoid recursion) ──

        public string ContentText
        {
            get { return rtb.Text; }
            set { rtb.Text = value; }
        }

        public Font EditorFont
        {
            get { return rtb.Font; }
            set
            {
                rtb.Font = value;
                gutter.Invalidate();
            }
        }

        public Color TextForeColor
        {
            get { return rtb.ForeColor; }
            set { rtb.ForeColor = value; }
        }

        public Color TextBackColor
        {
            get { return rtb.BackColor; }
            set { rtb.BackColor = value; }
        }

        public int ContentLength { get { return rtb.TextLength; } }

        public Color GutterBackColor
        {
            get { return gutter.BackColor; }
            set { gutter.BackColor = value; }
        }

        Color gutterForeColor = Color.FromArgb(130, 130, 130);
        public Color GutterForeColor
        {
            get { return gutterForeColor; }
            set { gutterForeColor = value; gutter.Invalidate(); }
        }

        public void SelectAll() { rtb.SelectAll(); }

        public void SelectText(int start, int length)
        {
            rtb.Select(start, length);
            rtb.ScrollToCaret();
        }

        public void ScrollToCaret() { rtb.ScrollToCaret(); }

        public void Clear() { rtb.Clear(); }

        public new bool Focus() { return rtb.Focus(); }

        public void InsertAtCursor(string text)
        {
            rtb.SelectionLength = 0;
            rtb.SelectedText = text;
        }

        public int GetCurrentLineNumber()
        {
            return rtb.GetLineFromCharIndex(rtb.SelectionStart) + 1;
        }

        public int GetTotalLines()
        {
            int total = rtb.Lines.Length;
            return total > 0 ? total : 1;
        }

        public void GoToLine(int lineNumber)
        {
            int idx = rtb.GetFirstCharIndexFromLine(lineNumber - 1);
            if (idx >= 0)
            {
                rtb.SelectionStart = idx;
                rtb.SelectionLength = 0;
                rtb.ScrollToCaret();
                rtb.Focus();
            }
        }

        // ── Clipboard ──

        public void Cut()
        {
            if (rtb.SelectionLength > 0) rtb.Cut();
        }

        public void Copy()
        {
            if (rtb.SelectionLength > 0) rtb.Copy();
        }

        public void Paste()
        {
            if (Clipboard.ContainsText())
                PastePlainText();
        }

        public void PastePlainText()
        {
            if (!Clipboard.ContainsText()) return;
            string text = Clipboard.GetText(TextDataFormat.UnicodeText);
            if (text == null) text = Clipboard.GetText();
            if (text != null)
            {
                rtb.SelectedText = text;
            }
        }

        // ── Line operations ──

        public void DuplicateLine()
        {
            int line = rtb.GetLineFromCharIndex(rtb.SelectionStart);
            int start = rtb.GetFirstCharIndexFromLine(line);
            int nextStart = rtb.GetFirstCharIndexFromLine(line + 1);
            string lineText;
            if (nextStart < 0)
            {
                // Last line — no trailing newline
                lineText = rtb.Text.Substring(start);
                rtb.SelectionStart = rtb.TextLength;
                rtb.SelectionLength = 0;
                rtb.SelectedText = "\n" + lineText;
            }
            else
            {
                lineText = rtb.Text.Substring(start, nextStart - start);
                rtb.SelectionStart = nextStart;
                rtb.SelectionLength = 0;
                rtb.SelectedText = lineText;
            }
        }

        public void DeleteLine()
        {
            int line = rtb.GetLineFromCharIndex(rtb.SelectionStart);
            int start = rtb.GetFirstCharIndexFromLine(line);
            int nextStart = rtb.GetFirstCharIndexFromLine(line + 1);
            if (nextStart < 0)
            {
                // Last line
                if (start > 0) start--; // include the preceding newline
                rtb.Select(start, rtb.TextLength - start);
            }
            else
            {
                rtb.Select(start, nextStart - start);
            }
            rtb.SelectedText = "";
        }

        // ── Drag & drop ──

        void OnDragDrop(object sender, DragEventArgs e)
        {
            if (e.Data.GetDataPresent(DataFormats.FileDrop))
            {
                string[] files = (string[])e.Data.GetData(DataFormats.FileDrop);
                if (files != null && files.Length > 0)
                {
                    try
                    {
                        string content = System.IO.File.ReadAllText(files[0]);
                        rtb.SelectedText = content;
                    }
                    catch { }
                }
            }
            else if (e.Data.GetDataPresent(DataFormats.Text))
            {
                string text = (string)e.Data.GetData(DataFormats.Text);
                if (text != null)
                    rtb.SelectedText = text;
            }
        }

        // ── Occurrence highlighting ──

        void OnSelectionChanged()
        {
            if (suppressEvents) return;
            highlightTimer.Stop();
            highlightTimer.Start();
        }

        void ApplyOccurrenceHighlights()
        {
            string selectedText = rtb.SelectedText;

            // Trim trailing newlines that RichTextBox may include
            if (selectedText != null)
                selectedText = selectedText.TrimEnd('\r', '\n');

            // Determine if we should highlight
            bool shouldHighlight = selectedText != null
                && selectedText.Length >= MinHighlightLength
                && selectedText.IndexOf('\n') < 0
                && selectedText.IndexOf('\r') < 0
                && selectedText.Trim().Length == selectedText.Length;

            string word = shouldHighlight ? selectedText : null;

            // Skip if same word is already highlighted
            if (word == lastHighlightedWord
                || (word != null && lastHighlightedWord != null && word == lastHighlightedWord))
                return;

            suppressEvents = true;
            int savedSelStart = rtb.SelectionStart;
            int savedSelLength = rtb.SelectionLength;

            // Remove previous highlights
            ClearHighlights();

            if (word != null)
            {
                // Find all occurrences
                string text = rtb.Text;
                int idx = 0;
                while (idx < text.Length)
                {
                    idx = text.IndexOf(word, idx, StringComparison.OrdinalIgnoreCase);
                    if (idx < 0) break;
                    // Skip the current selection itself
                    if (idx != savedSelStart)
                    {
                        highlightPositions.Add(idx);
                        rtb.Select(idx, word.Length);
                        rtb.SelectionBackColor = Theme.MatchHighlight;
                    }
                    idx += word.Length;
                }
                lastHighlightedWord = word;
            }

            // Restore selection
            rtb.Select(savedSelStart, savedSelLength);
            suppressEvents = false;
        }

        void ClearHighlights()
        {
            if (highlightPositions.Count > 0 && lastHighlightedWord != null)
            {
                int wordLen = lastHighlightedWord.Length;
                for (int i = 0; i < highlightPositions.Count; i++)
                {
                    rtb.Select(highlightPositions[i], wordLen);
                    rtb.SelectionBackColor = rtb.BackColor;
                }
                highlightPositions.Clear();
            }
            lastHighlightedWord = null;
        }

        // ── Gutter painting ──

        void OnGutterPaint(object sender, PaintEventArgs e)
        {
            Graphics g = e.Graphics;
            g.Clear(gutter.BackColor);

            int totalLines = rtb.Lines.Length;
            if (totalLines == 0) totalLines = 1;

            // Adjust gutter width based on digit count
            int digits = totalLines.ToString().Length;
            int newWidth = Math.Max(40, digits * 10 + GutterPadding * 2 + 4);
            if (gutter.Width != newWidth)
            {
                gutter.Width = newWidth;
                return;
            }

            int firstCharIndex = rtb.GetCharIndexFromPosition(new Point(0, 0));
            int firstLine = rtb.GetLineFromCharIndex(firstCharIndex);

            float lineHeight = rtb.Font.GetHeight(g);
            int visibleLines = (int)(rtb.ClientSize.Height / lineHeight) + 2;

            for (int i = 0; i < visibleLines; i++)
            {
                int lineIndex = firstLine + i;
                if (lineIndex >= totalLines) break;

                int charIndex = rtb.GetFirstCharIndexFromLine(lineIndex);
                if (charIndex < 0) break;

                Point pos = rtb.GetPositionFromCharIndex(charIndex);
                if (pos.Y + lineHeight < 0) continue;
                if (pos.Y > rtb.ClientSize.Height) break;

                DrawLineNumber(g, lineIndex + 1, pos.Y);
            }
        }

        void DrawLineNumber(Graphics g, int number, float y)
        {
            string text = number.ToString();
            SizeF size = g.MeasureString(text, rtb.Font);
            float x = gutter.Width - size.Width - GutterPadding;
            using (var brush = new SolidBrush(gutterForeColor))
            {
                g.DrawString(text, rtb.Font, brush, x, y);
            }
        }
    }
}

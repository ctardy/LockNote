using System;
using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class GoToLineDialog : Form
    {
        TextBox txtLine;
        public int LineNumber { get; private set; }

        public GoToLineDialog(int currentLine, int totalLines)
        {
            Text = "Go to line";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(260, 100);

            var lbl = new Label
            {
                Text = string.Format("Line number (1 - {0}):", totalLines),
                Location = new Point(12, 12),
                AutoSize = true
            };

            txtLine = new TextBox
            {
                Location = new Point(12, 34),
                Width = 230,
                Text = currentLine.ToString()
            };
            txtLine.SelectAll();

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(86, 66),
                Width = 75
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(167, 66),
                Width = 75
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, txtLine, btnOK, btnCancel });

            btnOK.Click += (s, e) =>
            {
                int line;
                if (!int.TryParse(txtLine.Text, out line) || line < 1 || line > totalLines)
                {
                    MessageBox.Show(
                        string.Format("Please enter a number between 1 and {0}.", totalLines),
                        "Go to line", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    DialogResult = DialogResult.None;
                    return;
                }
                LineNumber = line;
            };
        }
    }
}

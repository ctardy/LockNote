using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    /// <summary>
    /// "Unsaved changes" dialog with a "Remember my choice" checkbox.
    /// Returns Yes (save), No (discard), or Cancel (go back).
    /// </summary>
    class CloseConfirmDialog : Form
    {
        CheckBox chkRemember;

        public bool RememberChoice { get { return chkRemember.Checked; } }

        public CloseConfirmDialog()
        {
            Text = "LockNote";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(400, 140);

            var lbl = new Label
            {
                Text = "Unsaved changes. Save before closing?",
                Location = new Point(20, 18),
                AutoSize = true
            };

            chkRemember = new CheckBox
            {
                Text = "Remember my choice (can be changed in Settings)",
                Location = new Point(20, 50),
                AutoSize = true
            };

            var btnYes = new Button
            {
                Text = "Save",
                DialogResult = DialogResult.Yes,
                Location = new Point(126, 95),
                Width = 85,
                Height = 30
            };
            var btnNo = new Button
            {
                Text = "Don't save",
                DialogResult = DialogResult.No,
                Location = new Point(217, 95),
                Width = 85,
                Height = 30
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(308, 95),
                Width = 85,
                Height = 30
            };

            AcceptButton = btnYes;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, chkRemember, btnYes, btnNo, btnCancel });

            Theme.ApplyToDialog(this);
        }
    }
}

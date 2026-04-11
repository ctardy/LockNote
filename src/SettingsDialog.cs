using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class SettingsDialog : Form
    {
        ComboBox cboCloseAction;

        public Settings Result { get; private set; }

        public SettingsDialog(Settings current)
        {
            Text = "LockNote - Settings";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(340, 110);

            var lbl = new Label
            {
                Text = "On close with unsaved changes:",
                Location = new Point(12, 15),
                AutoSize = true
            };

            cboCloseAction = new ComboBox
            {
                DropDownStyle = ComboBoxStyle.DropDownList,
                Location = new Point(12, 38),
                Width = 310
            };
            cboCloseAction.Items.AddRange(new object[]
            {
                "Ask me each time",
                "Always save automatically",
                "Never save (discard changes)"
            });
            cboCloseAction.SelectedIndex = (int)current.SaveOnClose;

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(166, 72),
                Width = 75
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(247, 72),
                Width = 75
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl, cboCloseAction, btnOK, btnCancel });

            btnOK.Click += (s, e) =>
            {
                Result = new Settings();
                Result.SaveOnClose = (CloseAction)cboCloseAction.SelectedIndex;
            };
        }
    }
}

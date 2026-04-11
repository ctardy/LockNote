using System.Drawing;
using System.Windows.Forms;

namespace LockNote
{
    class SettingsDialog : Form
    {
        ComboBox cboCloseAction;
        ComboBox cboTheme;

        public Settings Result { get; private set; }

        public SettingsDialog(Settings current)
        {
            Text = "LockNote - Settings";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ClientSize = new Size(380, 175);

            var lbl1 = new Label
            {
                Text = "On close with unsaved changes:",
                Location = new Point(20, 18),
                AutoSize = true
            };

            cboCloseAction = new ComboBox
            {
                DropDownStyle = ComboBoxStyle.DropDownList,
                Location = new Point(20, 42),
                Width = 340
            };
            cboCloseAction.Items.AddRange(new object[]
            {
                "Ask me each time",
                "Always save automatically",
                "Never save (discard changes)"
            });
            cboCloseAction.SelectedIndex = (int)current.SaveOnClose;

            var lbl2 = new Label
            {
                Text = "Theme:",
                Location = new Point(20, 76),
                AutoSize = true
            };

            cboTheme = new ComboBox
            {
                DropDownStyle = ComboBoxStyle.DropDownList,
                Location = new Point(20, 98),
                Width = 340
            };
            cboTheme.Items.AddRange(new object[]
            {
                "Dark",
                "Light"
            });
            cboTheme.SelectedIndex = (int)current.ThemeMode;

            var btnOK = new Button
            {
                Text = "OK",
                DialogResult = DialogResult.OK,
                Location = new Point(196, 135),
                Width = 80,
                Height = 30
            };
            var btnCancel = new Button
            {
                Text = "Cancel",
                DialogResult = DialogResult.Cancel,
                Location = new Point(282, 135),
                Width = 80,
                Height = 30
            };

            AcceptButton = btnOK;
            CancelButton = btnCancel;
            Controls.AddRange(new Control[] { lbl1, cboCloseAction, lbl2, cboTheme, btnOK, btnCancel });

            btnOK.Click += (s, e) =>
            {
                Result = new Settings();
                Result.SaveOnClose = (CloseAction)cboCloseAction.SelectedIndex;
                Result.ThemeMode = (AppTheme)cboTheme.SelectedIndex;
            };

            Theme.ApplyToDialog(this);
        }
    }
}

// DropChoiceDialog — Choix lors d'un drag-and-drop de fichier
//
// Fichier texte : 3 boutons (Insérer le texte / Ajouter en pièce jointe / Annuler)
// Fichier binaire : 2 boutons (Ajouter en pièce jointe / Annuler)
// Retourne DropChoice { InsertText, Attach, Cancel }

use native_windows_gui as nwg;
use std::cell::RefCell;
use std::rc::Rc;

fn t(key: &str) -> String {
    crate::features::i18n::get(key).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropChoice {
    InsertText,
    Attach,
    Cancel,
}

pub struct DropChoiceDialog;

impl DropChoiceDialog {
    /// Affiche le dialog.
    ///
    /// `file_name` : nom du fichier affiché dans le message.
    /// `is_text` : si true, propose les 3 options ; sinon seulement Attach/Cancel.
    pub fn show(file_name: &str, is_text: bool) -> DropChoice {
        let height = if is_text { 180 } else { 160 };

        let mut window = Default::default();
        nwg::Window::builder()
            .size((460, height))
            .position((300, 300))
            .title(&t("drop.choice.title"))
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .build(&mut window)
            .expect("Failed to build DropChoiceDialog window");

        super::apply_dialog_theme(&window);

        let mut font = Default::default();
        nwg::Font::builder()
            .family("Segoe UI")
            .size(16)
            .build(&mut font)
            .expect("Failed to build font");

        // Message principal
        let msg_key = if is_text { "drop.choice.text" } else { "drop.choice.binary" };
        let mut lbl_msg = Default::default();
        nwg::Label::builder()
            .text(&t(msg_key))
            .parent(&window)
            .position((20, 15))
            .size((420, 25))
            .font(Some(&font))
            .build(&mut lbl_msg)
            .expect("Failed to build message label");

        // Nom du fichier
        let mut lbl_file = Default::default();
        nwg::Label::builder()
            .text(file_name)
            .parent(&window)
            .position((20, 45))
            .size((420, 40))
            .font(Some(&font))
            .build(&mut lbl_file)
            .expect("Failed to build file label");

        // Boutons : placement adaptatif
        let btn_y = if is_text { 110 } else { 95 };

        let mut btn_insert = Default::default();
        let mut btn_attach = Default::default();
        let mut btn_cancel = Default::default();

        if is_text {
            nwg::Button::builder()
                .text(&t("drop.choice.insert"))
                .parent(&window)
                .position((20, btn_y))
                .size((130, 30))
                .font(Some(&font))
                .build(&mut btn_insert)
                .expect("Failed to build Insert button");

            nwg::Button::builder()
                .text(&t("drop.choice.attach"))
                .parent(&window)
                .position((160, btn_y))
                .size((150, 30))
                .font(Some(&font))
                .build(&mut btn_attach)
                .expect("Failed to build Attach button");

            nwg::Button::builder()
                .text(&t("drop.choice.cancel"))
                .parent(&window)
                .position((330, btn_y))
                .size((110, 30))
                .font(Some(&font))
                .build(&mut btn_cancel)
                .expect("Failed to build Cancel button");
        } else {
            nwg::Button::builder()
                .text(&t("drop.choice.attach"))
                .parent(&window)
                .position((160, btn_y))
                .size((150, 30))
                .font(Some(&font))
                .build(&mut btn_attach)
                .expect("Failed to build Attach button");

            nwg::Button::builder()
                .text(&t("drop.choice.cancel"))
                .parent(&window)
                .position((330, btn_y))
                .size((110, 30))
                .font(Some(&font))
                .build(&mut btn_cancel)
                .expect("Failed to build Cancel button");
        }

        // Enter key → défaut conditionnel :
        //   - fichier texte : Insert (préserve le comportement historique du drag-drop)
        //   - fichier binaire : Attach (seul choix constructif restant)
        let default_btn = if is_text { btn_insert.handle } else { btn_attach.handle };
        super::bind_enter_to_button(window.handle, default_btn);

        let result: Rc<RefCell<DropChoice>> = Rc::new(RefCell::new(DropChoice::Cancel));
        let window_handle = window.handle;
        let result_clone = result.clone();

        let btn_insert_handle = btn_insert.handle;
        let btn_attach_handle = btn_attach.handle;
        let btn_cancel_handle = btn_cancel.handle;

        let handler = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnButtonClick => {
                    if is_text && handle == btn_insert_handle {
                        *result_clone.borrow_mut() = DropChoice::InsertText;
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_attach_handle {
                        *result_clone.borrow_mut() = DropChoice::Attach;
                        nwg::stop_thread_dispatch();
                    } else if handle == btn_cancel_handle {
                        nwg::stop_thread_dispatch();
                    }
                }
                nwg::Event::OnWindowClose => {
                    nwg::stop_thread_dispatch();
                }
                _ => {}
            }
        });

        nwg::dispatch_thread_events();
        nwg::unbind_event_handler(&handler);

        let r = { *result.borrow() };
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_choice_variants_are_distinct() {
        assert_ne!(DropChoice::InsertText, DropChoice::Attach);
        assert_ne!(DropChoice::Attach, DropChoice::Cancel);
        assert_ne!(DropChoice::InsertText, DropChoice::Cancel);
    }
}

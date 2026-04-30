use std::cell::{Cell, OnceCell, RefCell};

use adw::gtk::CompositeTemplate;
use adw::prelude::*;
use adw::subclass::prelude::*;
use adw::{gio, glib, gtk};
use glib::Properties;
use glib::subclass::InitializingObject;

const LOG_DOMAIN: &str = "syncbus-folder-row";

mod imp {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[derive(CompositeTemplate, Default, Properties)]
    #[properties(wrapper_type = super::FolderRow)]
    #[template(resource = "/mobi/phosh/syncbus/demo/ui/folder_row.ui")]
    pub struct FolderRow {
        #[template_child]
        image: TemplateChild<gtk::Image>,

        #[property(get, construct_only)]
        proxy: RefCell<Option<gio::DBusProxy>>,

        paused: Cell<bool>,
        cancellable: OnceCell<gio::Cancellable>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FolderRow {
        const NAME: &'static str = "SyncbusFolderRow";
        type Type = super::FolderRow;
        type ParentType = adw::ActionRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for FolderRow {
        fn constructed(&self) {
            self.parent_constructed();

            self.cancellable.set(gio::Cancellable::new()).unwrap();

            let binding = self.proxy.borrow();
            let proxy = binding.as_ref().unwrap();

            proxy.connect_closure(
                "g-properties-changed",
                false,
                glib::closure_local!(
                    #[weak(rename_to = this)]
                    self,
                    move |_: &gio::DBusProxy, _: glib::Variant, _: Vec<String>| this
                        .on_properties_changed()
                ),
            );
            self.on_properties_changed();
        }

        fn dispose(&self) {
            self.cancellable.get().unwrap().cancel();
        }
    }

    impl WidgetImpl for FolderRow {}

    impl ListBoxRowImpl for FolderRow {}

    impl PreferencesRowImpl for FolderRow {}

    impl ActionRowImpl for FolderRow {}

    #[gtk::template_callbacks]
    impl FolderRow {
        fn on_properties_changed(&self) {
            let binding = self.proxy.borrow();
            let proxy = binding.as_ref().unwrap();

            let label = if let Some(value) = proxy.cached_property("Label") {
                <String>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Label` not found in proxy");
                return;
            };

            let path = if let Some(value) = proxy.cached_property("Path") {
                <String>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Path` not found in proxy");
                return;
            };

            let completion = if let Some(value) = proxy.cached_property("Completion") {
                <u8>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Completion` not found in proxy");
                return;
            };

            let mut state = if let Some(value) = proxy.cached_property("State") {
                <String>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `State` not found in proxy");
                return;
            };
            if state.is_empty() {
                state = String::from("unknown");
            }

            let paused = if let Some(value) = proxy.cached_property("Paused") {
                <bool>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Paused` not found in proxy");
                return;
            };

            let icon_name = if paused {
                "play-symbolic"
            } else {
                "stop-symbolic"
            };

            self.obj().set_title(&label);
            let subtitle = format!("{path}\nCompletion: {completion}%\nState: {state}");
            self.obj().set_subtitle(&subtitle);
            self.image.set_icon_name(Some(icon_name));
            self.paused.set(paused);
        }

        #[template_callback]
        fn on_activated(&self, _row: adw::ActionRow) {
            let binding = self.proxy.borrow();
            let proxy = binding.as_ref().unwrap();

            let params = (!self.paused.get(),).to_variant();

            proxy.call(
                "SetPaused",
                Some(&params),
                gio::DBusCallFlags::NONE,
                -1,
                self.cancellable.get(),
                move |result: Result<glib::Variant, glib::Error>| {
                    if result.is_ok() {
                        return;
                    }

                    let error = result.err().unwrap();
                    glib::g_critical!(LOG_DOMAIN, "SetPaused failed: {error}");
                },
            );
        }
    }
}

glib::wrapper! {
    pub struct FolderRow(ObjectSubclass<imp::FolderRow>)
        @extends adw::ActionRow, adw::PreferencesRow, gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl FolderRow {
    #[must_use]
    pub fn new(proxy: &gio::DBusProxy) -> Self {
        glib::Object::builder().property("proxy", proxy).build()
    }
}

impl Default for FolderRow {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

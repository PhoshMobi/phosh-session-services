use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;

use adw::gtk::CompositeTemplate;
use adw::prelude::*;
use adw::subclass::prelude::*;
use adw::{gio, glib, gtk};
use glib::subclass::InitializingObject;

use crate::FolderRow;

const LOG_DOMAIN: &str = "syncbus-window";

const SYNCBUS_BUS_NAME: &str = "mobi.phosh.syncbus";
const SYNCBUS_MANAGER_IFACE: &str = "mobi.phosh.syncbus.Manager";
const SYNCBUS_MANAGER_PATH: &str = "/mobi/phosh/syncbus/manager";
const SYNCBUS_FOLDER_IFACE: &str = "mobi.phosh.syncbus.Folder";
const SYNCBUS_FOLDERS_PATH: &str = "/mobi/phosh/syncbus/folders";

mod imp {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/mobi/phosh/syncbus/demo/ui/window.ui")]
    pub struct Window {
        #[template_child]
        enable_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        enable_img: TemplateChild<gtk::Image>,
        #[template_child]
        error_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        url_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        folders_group: TemplateChild<adw::PreferencesGroup>,

        enabled: Cell<bool>,
        cancellable: OnceCell<gio::Cancellable>,
        proxy: OnceCell<gio::DBusProxy>,
        manager: OnceCell<gio::DBusObjectManagerClient>,

        folders: RefCell<HashMap<glib::GString, FolderRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "SyncbusWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();

            self.cancellable.set(gio::Cancellable::new()).unwrap();

            gio::DBusProxy::for_bus(
                gio::BusType::Session,
                gio::DBusProxyFlags::NONE,
                None,
                SYNCBUS_BUS_NAME,
                SYNCBUS_MANAGER_PATH,
                SYNCBUS_MANAGER_IFACE,
                self.cancellable.get(),
                glib::clone!(
                    #[weak(rename_to = this)]
                    self,
                    move |result| this.on_proxy_ready(result)
                ),
            );
        }

        fn dispose(&self) {
            self.cancellable.get().unwrap().cancel();
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {}

    impl ApplicationWindowImpl for Window {}

    impl AdwApplicationWindowImpl for Window {}

    #[gtk::template_callbacks]
    impl Window {
        fn on_properties_changed(&self) {
            let proxy = self.proxy.get().unwrap();

            let enabled = if let Some(value) = proxy.cached_property("Enabled") {
                <bool>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(
                    LOG_DOMAIN,
                    "Property `Enabled` not found in proxy; is the Syncbus D-Bus server running?"
                );
                self.obj().close();
                return;
            };

            let error = if let Some(value) = proxy.cached_property("Error") {
                <String>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Error` not found in proxy");
                self.obj().close();
                return;
            };

            let url = if let Some(value) = proxy.cached_property("Url") {
                <String>::from_variant(&value).unwrap()
            } else {
                glib::g_critical!(LOG_DOMAIN, "Property `Url` not found in proxy");
                self.obj().close();
                return;
            };

            let (title, subtitle, icon_name) = if enabled {
                (
                    "Disable Syncthing",
                    "Disable Syncthing through Systemd.",
                    "stop-symbolic",
                )
            } else {
                (
                    "Enable Syncthing",
                    "Enable Syncthing through Systemd.",
                    "play-symbolic",
                )
            };

            self.enable_row.set_title(title);
            self.enable_row.set_subtitle(subtitle);
            self.enable_img.set_icon_name(Some(icon_name));
            self.enabled.set(enabled);

            let subtitle = if error.is_empty() {
                String::from("No errors set.")
            } else {
                error
            };
            self.error_row.set_subtitle(&subtitle);

            self.url_row.set_sensitive(enabled);
            self.url_row.set_subtitle(&url);

            self.folders_group.set_visible(enabled);
        }

        fn on_object_added(&self, object: &gio::DBusObject) {
            let path = object.object_path();
            glib::g_debug!(LOG_DOMAIN, "Adding folder for path: {path}");
            let interface = DBusObjectExt::interface(object, SYNCBUS_FOLDER_IFACE).unwrap();
            let proxy = interface.dynamic_cast_ref::<gio::DBusProxy>().unwrap();
            let row = FolderRow::new(proxy);
            let mut folders = self.folders.borrow_mut();
            self.folders_group.add(&row);
            folders.insert(path, row);
        }

        fn on_object_removed(&self, object: &gio::DBusObject) {
            let path = object.object_path();
            glib::g_debug!(LOG_DOMAIN, "Removing folder for path: {path}");
            let mut folders = self.folders.borrow_mut();
            let row = folders.remove(&path).unwrap();
            self.folders_group.remove(&row);
        }

        fn on_proxy_ready(&self, result: Result<gio::DBusProxy, glib::Error>) {
            let proxy = match result {
                Ok(proxy) => proxy,
                Err(error) => {
                    glib::g_critical!(LOG_DOMAIN, "Failed to load Syncbus proxy: {error}");
                    self.obj().close();
                    return;
                }
            };
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
            self.proxy.set(proxy).unwrap();
            self.on_properties_changed();

            glib::spawn_future_local(glib::clone!(
                #[weak(rename_to = this)]
                self,
                async move {
                    let result = gio::DBusObjectManagerClient::new_for_bus_future(
                        gio::BusType::Session,
                        gio::DBusObjectManagerClientFlags::NONE,
                        SYNCBUS_BUS_NAME,
                        SYNCBUS_FOLDERS_PATH,
                    )
                    .await;
                    let manager = match result {
                        Ok(manager) => manager,
                        Err(error) => {
                            glib::g_critical!(LOG_DOMAIN, "Failed to load manager proxy: {error}");
                            this.obj().close();
                            return;
                        }
                    };

                    manager.connect_object_added(glib::clone!(
                        #[weak]
                        this,
                        move |_, object| this.on_object_added(object)
                    ));
                    manager.connect_object_removed(glib::clone!(
                        #[weak]
                        this,
                        move |_, object| this.on_object_removed(object)
                    ));

                    for object in manager.objects() {
                        this.on_object_added(&object);
                    }

                    this.manager.set(manager).unwrap();
                }
            ));
        }

        fn on_enable_call_ready(&self, result: Result<glib::Variant, glib::Error>) {
            if result.is_ok() {
                return;
            }

            let error = result.err().unwrap();
            self.error_row.set_subtitle(&error.to_string());
            glib::g_critical!(LOG_DOMAIN, "Enable failed: {error}");
        }

        #[template_callback]
        fn on_enable_activated(&self, _row: adw::ActionRow) {
            let proxy = self.proxy.get().unwrap();

            let method = if self.enabled.get() { "Stop" } else { "Start" };

            proxy.call(
                method,
                None,
                gio::DBusCallFlags::NONE,
                -1,
                self.cancellable.get(),
                glib::clone!(
                    #[weak(rename_to = this)]
                    self,
                    move |result| this.on_enable_call_ready(result)
                ),
            );
        }

        #[template_callback]
        fn on_url_activated(&self, _row: adw::ActionRow) {
            let url = self.url_row.subtitle().unwrap();
            gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE).unwrap();
        }
    }
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Window {
    #[must_use]
    pub fn new(application: &adw::Application) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }
}

impl Default for Window {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

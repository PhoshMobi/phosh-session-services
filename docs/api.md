# API

This documents the D-Bus API of Syncbus.

## Bus Name

Syncbus is available at `mobi.phosh.syncbus` bus name in user's session bus.

## `mobi.phosh.syncbus.Manager`

The manager is the main entry object of interaction with Syncbus. It is
available at `/mobi/phosh/syncbus/manager`. When the Syncthing Systemd unit is
active, it tries to establish connection with it. If the connection
consecutively fails after some reasonable number of retries, the server exits
with an error.

### `Enabled: boolean`

Describes whether the Systemd Syncthing unit is enabled or not.

### `Error: string`

Describes the last error faced by Syncbus while interacting with Syncthing. The
string will be empty on no error.

### `Url: string`

The URL to Syncthing GUI.

### `Start(): void`

Starts the Systemd Syncthing unit.

### `Stop(): void`

Stops the Systemd Syncthing unit.

## `mobi.phosh.syncbus.Folder`

A folder of this interface is a folder managed by Syncthing. All the folders
available are exposed through the [object
manager](https://dbus.freedesktop.org/doc/dbus-specification.html#standard-interfaces-objectmanager)
at `/mobi/phosh/syncbus/folders`.

### `Id: string`

Identifier of the folder as per Syncthing.

### `Label: string`

Label of the folder.

### `Path: string`

Path to the folder.

### `Completion: number`

Completion status of the folder as a non-negative integer percentage.

### `State: string`

State of the folder as per Syncthing.

### `Paused: boolean`

If the folder is paused from activity.

### `SetPaused(paused: boolean): void`

Changes the paused status of the folder to `paused`.

# Changelog

All notable changes to Soplos Theme Manager are documented in this file.

## [3.0.0] - 2026-11-16

First stable release of the 3.0 rewrite: the program is no longer in beta.

### Added

- Metainfo for the application stores (`org.soplos.thememanager.metainfo.xml`) and the manual page (`soplos-theme-manager.1`), both installed by the package.
- Eight screenshots of the program, in English, in `src-tauri/resources/screenshots/`, in the README and in the metainfo: the six of the beta, taken again, and the new pages of Icons and Cursors.

### Changed

- GTK Themes, Icons and Cursors pages: the list is a grid of cards, as in Look Manager. A GTK theme is drawn with its own colors (or with the picture it brings, when its colors cannot be read), an icon theme with six of its icons and a cursor theme with four of its cursors. A click on a card applies the theme at once: there is no Activate button, and a trash button on each card deletes it. The pictures of icon and cursor themes are asked for one theme at a time, so the cards show at once.
- Applying a saved theme sets the look (GTK theme, icons, cursor and font) first and restarts the panel after it, so the panel starts once, with the final themes.
- The six base themes that come with the program are new: they use the Tela Soplos icon themes (`tela-soplos-dark` and `tela-soplos-light`) and carry the Status Tray in the panel. They no longer carry the Breeze icon themes inside them, so the six files take 68 MB instead of 127 MB, and the package is about half the size. `Soplos-2.0` is now `soplos-2.0`.
- The text of the interface cannot be selected or copied, in the same way as the context menu is off. Only the fields where text is typed keep the selection.

### Fixed

- Wallpapers page: the cards stayed empty after the small copies were made, until the page was opened again. They are drawn as each copy is ready.
- Wallpapers page: the small copies of a picture with transparency (a PNG or an SVG) were saved grey. They are flattened on a dark background first, and the copies are made again.
- Login Screen page: the background and the default user picture show the small copies kept in the cache, not the picture of several megabytes.
- Panel page: the changes made in one panel were lost when going to another one. Each panel keeps its changes, a dot on the tab says which have not been applied, and "Save & Apply" applies all of them and restarts the panel once.
- Panel page: for a panel on a side, the size field says "Width", not "Height".

## [3.0.0-4-beta] - 2026-11-12

### Fixed

- Dock page: it only found the Docklike of one panel. With a dock in two panels, only the first one was read and saved. Every Docklike of the panels is found now, in numeric order, and when there is more than one the page has a selector, "Dock of panel N", to edit each one with its own applications.
- Wallpapers page: setting a wallpaper did nothing on a computer whose monitors had never had one set from Xfce, because the program only changed the settings that xfdesktop already had. It looks for the connected monitors (`xrandr --listmonitors`) and makes the setting of each monitor and workspace that is missing. A style of "none" is changed to "zoomed", or no picture would show. When it fails, the error says why.

### Added

- Panel page: a Docklike that is added to a panel starts as a copy of the dock that already exists (the one of its own panel, or else the first), with its applications and settings, and not empty. Before, the first dock had to be removed to pin the applications again.
- Wallpapers page: the cards show a small copy of 400 pixels of each picture, made once with `gdk-pixbuf` and kept in `~/.cache/soplos-theme-manager/thumbs`, and not the picture of several megabytes. The list of each folder and the copies are also kept while the program runs, so coming back to the page shows them at once.
- Wallpapers page: installing asks where to put the pictures: only for the user (`~/.local/share/backgrounds`, no password) or for every user (`/usr/share/backgrounds`, with the administrator password), in a subfolder if wanted. Afterwards the page opens the folder they were put in.

### Changed

- Errors of setting or installing a wallpaper are shown in a notice, and when the administrator step fails, it says what pkexec or the step said and not a generic message.

## [3.0.0-3-beta] - 2026-11-07

### Fixed

- Login Screen page: after saving the page, the login screen stopped following the wallpaper. The page read a missing `user-background` key as off and wrote `false` when saving, but the LightDM GTK greeter takes a missing key as on. A missing key is now read as on.

### Changed

- Login Screen page: a new option, "Login background", chooses where the login screen takes its background from: "The one I choose", with the button to pick a picture, or "The same as the desktop". With the second one the preview shows the wallpaper of the desktop, the button to pick a picture is off, and saving gives the wallpaper that Xfce shows now to AccountsService, so the login screen shows the same one even when it was changed from the Xfce settings. It takes the place of the switch "Use each user's own wallpaper".

## [3.0.0-2-beta] - 2026-11-06

### Changed

- Wallpapers page: when a picture cannot be set, the status line shows the real error and not a generic message.

## [3.0.0-1-beta] - 2026-11-06

### Fixed

- Panel page: a panel placed on the left or the right edge was drawn in the middle of the screen. The values that the program wrote and read for the position of the panel were wrong: `p=10` is the bottom center of the screen in Xfce, not the left edge. The left edge is now `p=5` and the right edge `p=1`, and a corner (`p=6`, `p=8`) is read as an edge by the mode of the panel.
- Panel page: choosing the left or the right edge sets the mode of the panel to vertical, and choosing the top or the bottom sets it to horizontal. A deskbar is left as it was chosen. The change is also made when the settings are applied, so it does not depend on the window.

### Added

- Wallpapers page: choosing a picture puts it on the desktop at once, as the Xfce settings do. The button to set it stays.
- Login Screen page: the picture of the user. The button "My picture" asks first, copies the picture to `~/.face` and gives it to AccountsService, so Mugshot and the login screen show the new one. It cannot be bigger than 1 MiB, the limit of AccountsService.
- Login Screen page: the wallpaper set in the Wallpapers page, or by applying a theme, is also given to AccountsService, so the option to use the wallpaper of each user has something to show. The option has a hint that says where the wallpaper comes from.

### Changed

- Login Screen page: the button that was "Change user image" is now "Default image", with a hint: it is the picture the login screen shows for users who have none of their own.
- Login Screen page: the heights of the page are worked out again for the new row, so the content fits the window at its minimum height.

## [3.0.0-beta] - 2026-11-05

First beta: all the pages of 2.x are ported and the program is being tested.

### Added

- Six screenshots of the program, in English, in `src-tauri/resources/screenshots/` and in the README: Themes, Wallpapers, GTK Themes, Panel, Dock and Login Screen.

### Changed

- Themes page: the buttons are in one row and in two groups. Apply, Remove and Export act on the selected theme and are off while there is none; New and Install do not depend on the selection. Refresh is gone, because the list updates itself after each operation, and Restore base themes is in a menu at the end of the row. Export and Install say, in their tooltips, that they work with a `.sth` file.
- The six base themes that come with the program are new: `Soplos-2.0`, `soplos-classic-black`, `soplos-classic-white`, `soplos-modern-black`, `soplos-modern-white` and `soplos-unity`. `Soplos-2.0` carries its own wallpaper inside the file.
- Exporting a theme compresses faster: a theme of about 30 MB went from 9 s to 2 s.
- Applying a theme that has a panel looks for the Docklike plugin with one query to xfconf instead of one for each plugin number, and the panel is only asked to quit when it answers on the session bus. Before, `xfce4-panel --quit` opened an error window when there was no panel to close.

## [3.0.0-alpha] - 2026-11-05

### Added

- Skeleton of the 3.0 version, a rewrite of the 2.x program (Python 3 and GTK3) with Tauri 2, Rust and Svelte.
- Window with no frame of the system: a card with rounded corners and its own minimize, maximize and close buttons.
- Sidebar with the pages of 2.x: Themes, Wallpapers, GTK Themes, Icons, Cursors, Panel, Dock and Login Screen. Icons & Cursors, one page in 2.x, are now two.
- GTK Themes, Icons and Cursors pages: the installed themes with the active one marked, activation, and installation from a package (`.tar.gz`, `.tgz`, `.tar.xz`, `.tar.bz2`, `.tar` or `.zip`) that holds one or several themes. The themes of a package are chosen one by one, and they are installed for the current user or for every user (one administrator password). A theme that already exists is never overwritten. The scripts of a package are never run. A GTK theme that brings window borders activates them together. Icon and cursor themes show the picture of their folder and of their arrow.
- A theme can be deleted from the GTK Themes, Icons and Cursors pages, after a confirmation. The active theme is never deleted. A theme of the user is removed at once; one installed for every user needs the administrator password. A system theme that came with a Debian package is refused, and the message says which package to remove instead: this is checked again as administrator.
- The themes of a package can be loose at its top (a theme with no folder around it takes the name of the package), in a folder, or in folders inside folders. An icon theme that shares folders with a sibling theme of the same package keeps those links, as Gruvbox Plus Light does with Gruvbox Plus Dark.
- An icon or cursor theme is told apart from a GTK metatheme by its `[Icon Theme]` section, so a GTK theme whose `index.theme` is a desktop entry (Agata, for example) is no longer offered as an icon theme. A package opened on the wrong page says what it is and where it belongs (GTK Themes, Icons or Cursors), and a package of cursors for Windows (`.cur` and `.ani`) says that Xfce cannot use them.
- The links of a `.zip` package come back as links. A zip keeps a link as an entry that holds where it points, and unpacking it as a plain file turned every linked icon into a text file (Material Black Mango Numix FLAT has 14,168 links). Links that point outside the package are dropped.
- Dock page: the preview of the dock is the editor. The applications are dragged from the list into the strip, to the place they are wanted, moved inside it by dragging them and dragged out of it to remove them; the same can be done with the keyboard (Alt+Left and Alt+Right move the selected icon, Delete removes it) and with the buttons. The strip is always one row: its icons are as big as the width of the window allows, up to a limit, and shrink or grow with it. It reads the pinned applications of the Docklike Taskbar plugin from the panel, shows the names in the language of the user and leaves out the applications that are not meant to be shown in Xfce. Saving writes the file of the plugin and restarts the panel. Without the plugin in the panel, the page says so.
- Login Screen page: the settings of the LightDM GTK greeter (`/etc/lightdm/lightdm-gtk-greeter.conf`). The background and the image of the user, with a preview of each. The GTK, icon and cursor themes, chosen among the installed ones (a theme that the file names but is not installed stays in the list). The font, as a family and a size. The place of the login box, chosen by clicking a cell of a nine cell grid drawn over a preview of the screen, with fine offsets in percent (the preview places the box as the greeter does: the point of the screen, moved by the anchor, kept on the screen and below the bar of the greeter; a position the grid cannot show, in pixels or with a size, is kept as the file has it until it is touched), and the switches for the avatar of the user and for the own wallpaper of each user, and the format of the clock. Saving needs the administrator password. Only the settings of the page are changed in the file, and the rest of it, comments included, is kept as it was: the file is replaced in one step.
- Panel page: the panels of Xfce, with the settings of each one and its plugins. A selector of panels, with buttons to add one and to delete one (it asks first, and the plugins that no other panel uses are deleted with it). A preview that draws the panel on a small screen. The position (edge, alignment, length and lock), the appearance (mode, height, icon size, rows and dark mode) and the behavior (auto-hide and auto-expand). The plugins of the panel in a list that is reordered by dragging and with buttons, and the plugins that can be added, with a search and a double click. Saving writes the settings and the list of plugins into xfconf and restarts the panel. The edge and the alignment of a floating panel are worked out from where it touches the screen, so a panel set to 80% wide at the start is read back as that, and a vertical panel shows top, center and bottom as its places. Taking out the last plugin of a panel empties the list.
- Wallpapers page: the pictures (`.jpg`, `.jpeg`, `.png`, `.webp`, `.bmp` and `.svg`) of a folder and of the folders inside it, in a gallery. It opens on the first of the usual folders that exists (`/usr/share/wallpapers`, `/usr/share/backgrounds`, `/usr/share/xfce4/backdrops`, `~/Pictures` and `~/Imágenes`), and another folder can be chosen. A picture is set as the wallpaper of every screen and workspace of Xfce. New pictures are installed into `/usr/share/backgrounds` for every user, with one administrator password. They are copied first to the cache of the program by the user, so a file the user cannot read can never be installed.
- Themes page: the complete looks of the desktop (GTK theme, icons, cursor, fonts, window borders, wallpaper and panel), kept in `~/.config/soplos-theme-manager/themes`, the same place the 2.x program used, so the themes already saved there are listed. A gallery with the preview of each theme and, in a corner of each card, small icons of what the theme carries. Apply (double click too), New (saves the current look, with an optional screenshot for the preview), Remove (asks first), Export, Install, Refresh and Restore base themes. The panel of a theme is applied by writing its settings through xfconf and restarting the panel, with no kill of the settings daemon as 2.x did; the applications pinned to the dock belong to the user and are kept. Export writes a `.sth` file with the theme, the GTK theme, the icon themes it inherits from, the cursor and the wallpaper; a file inside a system theme that the user cannot read is left out. Install checks that the file is a Soplos theme (`soplos-theme-bundle-v3`), unpacks it in the cache with the same limits as any package, copies the themes it carries for the current user or for every user (one administrator password) and never overwrites one that exists. The six base themes come with the program and are installed the first time it runs; Restore brings back the ones that were removed. A `.sth` file opened from the file manager starts its installation, in the window that is already open if there is one. The `.sth` type is registered with its own icon. F2 renames the selected theme; a name that is already taken is refused.
- Light and dark theme that follows the one of Xfce, with a button in the sidebar to switch it.
- Interface in eight languages: English, Spanish, German, French, Italian, Portuguese, Romanian and Russian.
- About dialog (F1), Ctrl+Q to quit, Ctrl+Tab and Ctrl+Shift+Tab to move between pages.

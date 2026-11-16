# Soplos Theme Manager 3.0

![Version](https://img.shields.io/badge/version-3.0.0-blue)
![License](https://img.shields.io/badge/license-GPL--3.0--or--later-green)
![Platform](https://img.shields.io/badge/platform-Linux-lightgrey)
![Tauri](https://img.shields.io/badge/Tauri-2-orange)
![Rust](https://img.shields.io/badge/Rust-1.80+-orange)
![Svelte](https://img.shields.io/badge/Svelte-4-red)

Theme manager for Soplos Linux Tyron (Xfce): themes, wallpapers, GTK themes, icons and cursors, panel, dock and login screen.

Version 3.0 is a rewrite of the 2.x program, written in Python 3 with GTK3, with Tauri 2: a Rust backend and a Svelte 4 and Tailwind CSS interface. All the pages of 2.x are ported. The pages are: Themes, Wallpapers, GTK Themes, Icons, Cursors, Panel, Dock and Login Screen.

| ![Themes](src-tauri/resources/screenshots/screenshot01.png) The complete desktop themes you have saved, each card with small icons of what the theme carries | ![Wallpapers](src-tauri/resources/screenshots/screenshot02.png) The wallpapers of a folder, set as the wallpaper of every screen |
|:---:|:---:|
| ![GTK Themes](src-tauri/resources/screenshots/screenshot03.png) The installed GTK themes drawn with their colors, with the active one marked | ![Panel](src-tauri/resources/screenshots/screenshot04.png) The position, appearance, behavior and plugins of the Xfce panel |
| ![Dock](src-tauri/resources/screenshots/screenshot05.png) The dock is edited by dragging its applications into the strip | ![Login Screen](src-tauri/resources/screenshots/screenshot06.png) The LightDM greeter: background, themes, font and where the login box goes |
| ![Icons](src-tauri/resources/screenshots/screenshot07.png) The installed icon themes, each with six of its icons | ![Cursors](src-tauri/resources/screenshots/screenshot08.png) The installed cursor themes, each with four of its cursors |

## Pages

- **Themes**: apply, save, delete, export and import complete desktop themes (`.sth` files), each card with small icons of what the theme carries.
- **Wallpapers**: browse the pictures of a folder, set one as the background of every screen and workspace, and install new ones for every user.
- **GTK Themes**: the installed GTK themes as cards drawn with their colors; a click applies one, and the program installs and deletes themes. A theme that brings colors for the window borders sets them too.
- **Icons**: the installed icon themes as cards with six of their icons; a click applies one, and the program installs and deletes themes.
- **Cursors**: the installed cursor themes as cards with four of their cursors; a click applies one, and the program installs and deletes themes.
- **Panel**: the panels of Xfce, their position, appearance and behavior, and their plugins.
- **Dock**: the pinned applications of the Docklike Taskbar, edited by dragging them on the preview of the dock.
- **Login Screen**: the settings of the LightDM GTK greeter: background, themes, font, position of the login box and clock.

## Development

```bash
npm install
npm run tauri dev
```

## Packaging

```bash
./package.sh
```

## License

GPL-3.0-or-later.

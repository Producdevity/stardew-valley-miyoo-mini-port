# Stardew Valley for Miyoo Mini

<img width="492" height="688" alt="Stardew Valley running on a Miyoo Mini" src="https://github.com/user-attachments/assets/607e7608-37ee-4dbe-b394-92888a6438b6" />

An OnionOS port of Stardew Valley for the Miyoo Mini and Miyoo Mini Plus.

Stardew Valley and its assets are not included. Compatibility builds
`1.6.14.24317` and `1.6.15.24356` are supported.

## Setup app

The setup app can sign in to Steam with a Steam Mobile QR code, download the
supported compatibility build and prepare the OnionOS package. You do not need
to find or download a depot yourself.

If setup fails, open **Show log** in the app and use **Copy** or **Save** when
reporting the problem.

Download the setup app for Windows, macOS or Linux from the release page.
Debian and Ubuntu packages are available as `.deb` files too.

For command-line setup, use the `prepare.sh` script described below.

The app uses Mono 6 and Docker to build the device-specific game files. On
Windows, both tools must be available inside WSL.

## Manual setup

### Download a supported depot

1. Own Stardew Valley on Steam and sign in to the Steam desktop client.
2. Open `steam://open/console` in a browser.
3. For `1.6.15.24356`, run:

   ```text
   download_depot 413150 413151 4848991934266309406
   ```

4. Steam prints the download location when it finishes.

Steam's console may refuse the older `1.6.14.24317` manifest. It remains
available through [DepotDownloader](https://github.com/SteamRE/DepotDownloader):

```sh
DepotDownloader -app 413150 -depot 413151 -manifest 5538941793102260869 -beta compatibility -qr
```

### Prepare the port

1. Download the release archive and extract it.
2. Copy the downloaded depot into the release's `gamefiles` folder.
3. Install [Mono 6](https://www.mono-project.com/download/stable/) and
   [Docker](https://docs.docker.com/get-docker/).
4. Run `./prepare.sh` from the extracted folder. On Windows, run it in WSL.
5. Copy `OnionOS-package/Roms` to the root of the Miyoo SD card.

On Windows, first [install WSL](https://learn.microsoft.com/windows/wsl/install).
Open WSL and run `sudo apt update && sudo apt install mono-devel`, then enable
that distribution under Docker Desktop's
[WSL integration settings](https://docs.docker.com/desktop/features/wsl/).

The copy of `prepare.sh` under [release-tools](release-tools) is here for review.
Use the one in the release archive; it needs files that are only shipped with
the release.

The port appears in OnionOS as **Stardew Valley for Miyoo Mini**.

## Source and licenses

The files written for this project are released under the [MIT License](LICENSE).
The rest of the port is not public yet.

Modified OpenAL Soft source and its build script are under
[third_party/openal-soft](third_party/openal-soft). The release archive includes
the license notices for every bundled runtime component.

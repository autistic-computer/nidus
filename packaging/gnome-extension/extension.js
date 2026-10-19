// Loads zflow's Shell code from the build folder named in `current`. GJS keeps a module for as
// long as its path, so a new build in a new folder runs as soon as the extension is turned off
// and on again, without a new login. Keep this file unchanged: Shell keeps it until the next
// login, so a change here would need one after all.
import Gio from 'gi://Gio';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class ZflowLoader extends Extension {
    enable() {
        const token = this._token = {};
        import(`./${this._build()}shell.js`).then(({Zflow}) => {
            // Turned off, or off and on again, while loading.
            if (token !== this._token) return;
            this._zflow = new Zflow();
            this._zflow.enable();
        }).catch(error => logError(error, 'zflow could not load'));
    }

    disable() {
        this._token = null;
        this._zflow?.disable();
        this._zflow = null;
    }

    // A checkout or an extensions.gnome.org copy has no build folder, only the files themselves.
    _build() {
        try {
            const [, bytes] = Gio.File.new_for_path(`${this.path}/current`).load_contents(null);
            const build = new TextDecoder().decode(bytes).trim();
            return /^build-[0-9a-f]+$/.test(build) ? `${build}/` : '';
        } catch {
            return '';
        }
    }
}

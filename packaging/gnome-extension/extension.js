import Gio from 'gi://Gio';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class NidLoader extends Extension {
    enable() {
        const token = this._token = {};
        import(`./${this._build()}shell.js`).then(({Nid}) => {
            if (token !== this._token) return;
            this._nid = new Nid();
            this._nid.enable();
        }).catch(error => logError(error, 'Nidus could not load'));
    }

    disable() {
        this._token = null;
        this._nid?.disable();
        this._nid = null;
    }

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

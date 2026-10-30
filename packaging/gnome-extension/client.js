import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const BUS = 'io.nid.Desktop';
const PATH = '/io/nid/Desktop';
export const API = 6;

export function compatible(snapshot) {
    return (snapshot?.api ?? 1) === API;
}

export function statusText(snapshot) {
    if (snapshot?.agent_error) return 'Nidus is not running';
    if (!compatible(snapshot)) return 'Update Nidus';
    return snapshot.status.title;
}

export function needsAttention(snapshot) {
    return !compatible(snapshot) || snapshot.health.some(row => row.level === 'error');
}

export class Client {
    constructor(changed, activate = false) {
        this._activate = activate;
        this._changed = changed;
        this._cancel = new Gio.Cancellable();
        this._timer = 0;
        this._polling = false;
        this.snapshot = null;
    }

    call(request, timeout = 7000) {
        const flags = request.command === 'snapshot' && !this._activate ? Gio.DBusCallFlags.NO_AUTO_START : Gio.DBusCallFlags.NONE;
        if (request.command === 'snapshot') this._activate = false;
        return new Promise((resolve, reject) => {
            Gio.DBus.session.call(BUS, PATH, BUS, 'Call',
                new GLib.Variant('(s)', [JSON.stringify(request)]),
                new GLib.VariantType('(s)'), flags, timeout, this._cancel,
                (connection, result) => {
                    try { resolve(JSON.parse(connection.call_finish(result).deep_unpack()[0])); }
                    catch (error) {
                        if (error instanceof GLib.Error) Gio.DBusError.strip_remote_error(error);
                        reject(error);
                    }
                });
        });
    }

    start() {
        this.refresh();
        this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1000, () => {
            this.refresh();
            return GLib.SOURCE_CONTINUE;
        });
    }

    async refresh() {
        if (this._polling || this._cancel.is_cancelled()) return;
        this._polling = true;
        try {
            this.snapshot = await this.call({command: 'snapshot'});
        } catch (error) {
            this.snapshot = {agent_error: true, error: `Open nid settings to start the desktop agent. ${error.message}`};
        } finally {
            this._polling = false;
        }
        if (!this._cancel.is_cancelled()) this._changed(this.snapshot);
    }

    destroy() {
        if (this._timer) GLib.Source.remove(this._timer);
        this._timer = 0;
        this._cancel.cancel();
    }
}

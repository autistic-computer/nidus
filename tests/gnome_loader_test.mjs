import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

// Run the shipped loader with a fake file system and a fake dynamic import.
const source = fs.readFileSync(new URL('../packaging/gnome-extension/extension.js', import.meta.url), 'utf8')
    .replace(/^import .*;\r?\n/gm, '').replace('export default class ', 'class ')
    .replace('import(', 'load(') + '\nglobalThis.Loader = ZflowLoader;';

function loader(current) {
    const loads = [];
    const started = [];
    const context = {
        TextDecoder,
        Extension: class {constructor() {this.path = '/ext';}},
        Gio: {File: {new_for_path(path) {
            return {load_contents() {
                assert.equal(path, '/ext/current');
                if (current === undefined) throw new Error('no such file');
                return [true, new TextEncoder().encode(current)];
            }};
        }}},
        logError(error) {throw error;},
        load(path) {
            let resolve;
            const loaded = new Promise(done => {resolve = done;});
            loads.push({path, resolve: () => resolve({Zflow: class {
                enable() {started.push(path);}
                disable() {started.push(`off ${path}`);}
            }})});
            return loaded;
        },
    };
    vm.runInNewContext(source, context);
    return {extension: new context.Loader(), loads, started};
}

const settle = () => new Promise(setImmediate);

// The installed build folder wins, and a new one is new code.
{
    const {extension, loads, started} = loader('build-0123456789abcdef\n');
    extension.enable();
    loads[0].resolve();
    await settle();
    assert.deepEqual(started, ['./build-0123456789abcdef/shell.js']);
    extension.disable();
    assert.deepEqual(started.at(-1), 'off ./build-0123456789abcdef/shell.js');
}

// A checkout, an extensions.gnome.org copy, or a strange name loads the files beside it.
for (const current of [undefined, '../../etc', 'build-xyz', '']) {
    const {extension, loads} = loader(current);
    extension.enable();
    assert.equal(loads[0].path, './shell.js', `current ${JSON.stringify(current)}`);
}

// Turned off, or off and on again, while loading: only the newest enable starts.
{
    const {extension, loads, started} = loader('build-aa');
    extension.enable();
    extension.disable();
    extension.enable();
    loads[0].resolve();
    loads[1].resolve();
    await settle();
    assert.deepEqual(started, ['./build-aa/shell.js']);
    extension.disable();
    extension.enable();
    extension.disable();
    loads[2].resolve();
    await settle();
    assert.equal(started.length, 2, 'a load that lands after disable starts nothing');
}

console.log('GNOME extension loader build folder, fallback and enable races checks passed');

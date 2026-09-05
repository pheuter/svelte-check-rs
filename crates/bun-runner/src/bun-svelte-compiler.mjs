import fs from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { argv, env, stdin, stdout } from 'node:process';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const protocolToken = argv[2];
if (!protocolToken) {
  console.error('svelte-check-rs bun runner missing protocol token');
  process.exit(2);
}
// Capture the writer before loading user code. A config may replace
// `process.stdout.write`, but it must not be able to swallow protocol frames.
const protocolWrite = stdout.write.bind(stdout);
const send = (value) => protocolWrite(`\x1e${protocolToken}\t${JSON.stringify(value)}\n`);
const workspaceRequire = createRequire(pathToFileURL(path.join(process.cwd(), 'package.json')));

// Resolve from the source file, including packages outside the CLI workspace.
// Only a genuinely absent package may fall back to the workspace compiler.
const compilerModules = new Map();
function hasSveltePackage(root) {
  for (let current = root; ; current = path.dirname(current)) {
    try {
      fs.lstatSync(path.join(current, 'node_modules/svelte'));
      return true;
    } catch (error) {
      if (!['ENOENT', 'ENOTDIR'].includes(error.code)) throw error;
    }
    if (path.dirname(current) === current) return false;
  }
}
async function loadCompiler(filename) {
  const base = filename ? path.dirname(path.resolve(filename)) : process.cwd();
  let compilerPath;
  try {
    compilerPath = requireFrom(base).resolve('svelte/compiler');
  } catch (error) {
    if (!['MODULE_NOT_FOUND', 'ERR_MODULE_NOT_FOUND'].includes(error.code) || hasSveltePackage(base)) throw error;
    compilerPath = workspaceRequire.resolve('svelte/compiler');
  }
  compilerPath = fs.realpathSync(compilerPath);
  if (!compilerModules.has(compilerPath)) {
    compilerModules.set(compilerPath, (async () => {
      // Import failures must propagate, never silently switch compiler versions.
      const mod = await import(pathToFileURL(compilerPath).href);
      if (typeof mod.compile !== 'function' || typeof mod.preprocess !== 'function') {
        throw new Error(`Invalid Svelte compiler: ${compilerPath}`);
      }
      const hash = createHash('sha256').update(compilerPath).update(fs.readFileSync(compilerPath));
      for (let dir = path.dirname(compilerPath); ; dir = path.dirname(dir)) {
        const manifest = path.join(dir, 'package.json');
        if (fs.existsSync(manifest)) {
          const content = fs.readFileSync(manifest, 'utf8');
          if (JSON.parse(content).name === 'svelte') { hash.update(content); break; }
        }
        if (path.dirname(dir) === dir) break;
      }
      return { ...mod, identity: hash.digest('hex') };
    })());
  }
  return compilerModules.get(compilerPath);
}

send({ ready: true });

// This follows @sveltejs/load-config 0.2.0. Intentional differences are kept
// here: Bun supports .ts/.mts configs without Node's feature gate; the worker
// always traverses and its lifetime replaces the public traverse/clearCache
// options; config and imported-module dependencies are returned for watch
// mode; and only the serializable subset consumed by Rust crosses the protocol.
// Requests execute serially, so the upstream process.chdir mutex is unnecessary.
const loadedConfigs = new Map();
const VITE_CONFIG_EXTENSIONS = ['js', 'mjs', 'ts', 'cjs', 'mts', 'cts'];
const SVELTE_CONFIG_EXTENSIONS = ['js', 'cjs', 'mjs', 'ts', 'mts'];

function findConfig(dir, basename, extensions) {
  for (const extension of extensions) {
    const candidate = path.join(dir, `${basename}.${extension}`);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

async function collectModuleDependencies(configPath) {
  const dependencies = new Set([path.resolve(configPath)]);
  collectStaticModuleDependencies(configPath, dependencies);
  try {
    const result = await Bun.build({
      entrypoints: [configPath],
      write: false,
      metafile: true,
      target: 'bun',
      packages: 'external'
    });
    const metafile = typeof result.metafile === 'string'
      ? JSON.parse(result.metafile)
      : result.metafile;
    for (const input of Object.keys(metafile?.inputs || {})) {
      // Bun metafile inputs are relative to the build's current directory,
      // not to the entrypoint's directory.
      dependencies.add(path.isAbsolute(input) ? input : path.resolve(input));
    }
  } catch {
    // Keep the entrypoint. It is still useful for fixing a broken config.
  }
  return [...dependencies];
}

const MODULE_EXTENSIONS = ['.js', '.mjs', '.cjs', '.ts', '.mts', '.cts', '.jsx', '.tsx'];

function importCandidates(importer, specifier) {
  let resolved;
  try {
    if (specifier.startsWith('file:')) {
      resolved = fileURLToPath(specifier);
    } else if (path.isAbsolute(specifier)) {
      resolved = specifier;
    } else if (specifier.startsWith('./') || specifier.startsWith('../')) {
      resolved = fileURLToPath(new URL(specifier, pathToFileURL(importer)));
    } else {
      return [];
    }
  } catch {
    return [];
  }

  resolved = path.resolve(resolved);
  if (path.extname(resolved)) return [resolved];
  return [
    resolved,
    ...MODULE_EXTENSIONS.map((extension) => `${resolved}${extension}`),
    ...MODULE_EXTENSIONS.map((extension) => path.join(resolved, `index${extension}`))
  ];
}

function transpilerLoader(modulePath) {
  switch (path.extname(modulePath)) {
    case '.ts':
    case '.mts':
    case '.cts':
      return 'ts';
    case '.tsx':
      return 'tsx';
    case '.jsx':
      return 'jsx';
    case '.js':
    case '.mjs':
    case '.cjs':
      return 'js';
    default:
      return null;
  }
}

function collectStaticModuleDependencies(configPath, dependencies) {
  const pending = [path.resolve(configPath)];
  const visited = new Set();
  while (pending.length > 0) {
    const modulePath = pending.pop();
    if (visited.has(modulePath)) continue;
    visited.add(modulePath);

    const loader = transpilerLoader(modulePath);
    if (!loader || !fs.existsSync(modulePath)) continue;

    let imports;
    try {
      const source = fs.readFileSync(modulePath, 'utf8');
      imports = new Bun.Transpiler({ loader }).scanImports(source);
    } catch {
      // The module itself remains watched, so repairing its syntax retries the
      // config and discovers any dependencies that could not be scanned.
      continue;
    }

    for (const imported of imports) {
      const candidates = importCandidates(modulePath, imported.path);
      const existing = candidates.filter((candidate) => {
        try {
          return fs.statSync(candidate).isFile();
        } catch {
          return false;
        }
      });
      const watched = existing.length > 0 ? existing : candidates;
      for (const candidate of watched) {
        dependencies.add(candidate);
        if (existing.includes(candidate)) pending.push(candidate);
      }
    }
  }
}

async function loadSvelteConfig(configPath) {
  const dependencies = await collectModuleDependencies(configPath);
  try {
    const mod = await import(pathToFileURL(configPath).href);
    const config = mod?.default;
    if (!config) {
      throw new Error(
        'Missing exports in the config. Make sure to include "export default config" or "module.exports = config"'
      );
    }
    return {
      config,
      configFilePath: configPath,
      configSource: 'svelte',
      dependencies
    };
  } catch (error) {
    return { error, configFilePath: configPath, configSource: 'svelte', dependencies };
  }
}

function requireFrom(root) {
  return createRequire(pathToFileURL(path.join(root, 'package.json')));
}

function resolvePackageImportExport(exportEntry) {
  if (typeof exportEntry === 'string') return exportEntry;
  if (!exportEntry || typeof exportEntry !== 'object') return null;
  if (typeof exportEntry.import === 'string') return exportEntry.import;
  if (typeof exportEntry.import?.default === 'string') return exportEntry.import.default;
  return typeof exportEntry.default === 'string' ? exportEntry.default : null;
}

function getViteImportPath(root) {
  const localRequire = requireFrom(root);
  const packagePath = localRequire.resolve('vite/package.json');
  const packageDirectory = path.dirname(packagePath);
  const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  const entry = resolvePackageImportExport(packageJson.exports?.['.']);
  const target = entry ?? packageJson.module ?? packageJson.main;
  return target ? path.join(packageDirectory, target) : null;
}

async function importViteLegacy(root) {
  try {
    const main = requireFrom(root).resolve('vite');
    const previous = env.VITE_CJS_IGNORE_WARNING;
    env.VITE_CJS_IGNORE_WARNING = 'true';
    try {
      return await import(pathToFileURL(main).href);
    } finally {
      if (previous === undefined) delete env.VITE_CJS_IGNORE_WARNING;
      else env.VITE_CJS_IGNORE_WARNING = previous;
    }
  } catch {
    return null;
  }
}

async function importVite(root) {
  try {
    const importPath = getViteImportPath(root);
    if (importPath) return await import(pathToFileURL(importPath).href);
  } catch {
    // Fall through to Vite's legacy CommonJS entry.
  }
  return importViteLegacy(root);
}

async function collectViteDependencies(resolved, root, configPath, options) {
  // Vite's native loader can return no dependency list. Track imported local
  // modules ourselves so watch mode still reloads the entire config graph.
  const dependencies = new Set(await collectModuleDependencies(configPath));
  for (const dependency of resolved.configFileDependencies || []) {
    dependencies.add(path.resolve(dependency));
  }

  let svelteConfigPath = null;
  if (typeof options?.configFile === 'string') {
    svelteConfigPath = path.resolve(root, options.configFile);
  } else if (options?.configFile !== false) {
    svelteConfigPath = findConfig(root, 'svelte.config', SVELTE_CONFIG_EXTENSIONS);
  }
  if (svelteConfigPath) {
    for (const dependency of await collectModuleDependencies(svelteConfigPath)) {
      dependencies.add(dependency);
    }
  }
  return [...dependencies];
}

async function loadViteConfig(root, configPath) {
  const vite = await importVite(root);
  if (!vite?.resolveConfig) return null;
  const cwd = process.cwd();
  try {
    process.chdir(root);
    const resolved = await vite.resolveConfig(
      // Bun can execute TypeScript configs directly. Bundling under Vite 8
      // rewrites import.meta.resolve to virtual modules that require Node's
      // module.registerHooks, which Bun does not implement.
      { root, configFile: configPath, logLevel: 'error', configLoader: 'native' },
      'serve'
    );
    const kitOptions = resolved.plugins.find(
      (plugin) => plugin.name === 'vite-plugin-sveltekit-setup'
    )?.api?.options;
    if (kitOptions) {
      const { preprocess, compilerOptions, extensions, vitePlugin, ...kit } = kitOptions;
      return {
        config: { preprocess, compilerOptions, extensions, vitePlugin, kit },
        configFilePath: configPath,
        configSource: 'vite',
        dependencies: await collectViteDependencies(resolved, root, configPath, kitOptions)
      };
    }
    const options = resolved.plugins.find(
      (plugin) => plugin.name === 'vite-plugin-svelte:config'
    )?.api?.options;
    if (options) {
      return {
        config: options,
        configFilePath: configPath,
        configSource: 'vite',
        dependencies: await collectViteDependencies(resolved, root, configPath, options)
      };
    }
    return null;
  } catch (error) {
    return {
      error,
      configFilePath: configPath,
      configSource: 'vite',
      dependencies: await collectModuleDependencies(configPath)
    };
  } finally {
    process.chdir(cwd);
  }
}

async function loadConfigFromDirectory(dir) {
  const vitePath = findConfig(dir, 'vite.config', VITE_CONFIG_EXTENSIONS);
  let viteError = null;
  if (vitePath) {
    const result = await loadViteConfig(dir, vitePath);
    if (result?.config) return result;
    if (result?.error) viteError = result;
  }
  const sveltePath = findConfig(dir, 'svelte.config', SVELTE_CONFIG_EXTENSIONS);
  if (!sveltePath) return viteError;

  const svelteResult = await loadSvelteConfig(sveltePath);
  if (viteError) {
    svelteResult.dependencies = [
      ...new Set([...viteError.dependencies, ...svelteResult.dependencies])
    ];
  }
  return svelteResult;
}

async function loadEffectiveConfig(root, configPath = null) {
  root = path.resolve(root);
  configPath = configPath ? path.resolve(configPath) : null;
  const key = configPath || root;
  if (!loadedConfigs.has(key)) {
    loadedConfigs.set(key, (async () => {
      if (configPath) {
        if (path.basename(configPath).startsWith('svelte.config.')) {
          return loadSvelteConfig(configPath);
        }
        const viteResult = await loadViteConfig(path.dirname(configPath), configPath);
        return viteResult ?? loadSvelteConfig(configPath);
      }
      let current = root;
      while (true) {
        const result = await loadConfigFromDirectory(current);
        if (result) return result;
        const parent = path.dirname(current);
        if (parent === current) return null;
        current = parent;
      }
    })());
  }
  return loadedConfigs.get(key);
}

function serializableConfig(result) {
  if (!result) return { found: false };
  if (result.error) {
    return {
      found: true,
      configFilePath: result.configFilePath,
      configSource: result.configSource,
      dependencies: result.dependencies || [],
      error: result.error?.message || String(result.error)
    };
  }
  const config = result.config || {};
  const preprocessValue = config.preprocess;
  return {
    found: true,
    configFilePath: result.configFilePath,
    configSource: result.configSource,
    dependencies: result.dependencies || [],
    hasPreprocess: Array.isArray(preprocessValue)
      ? preprocessValue.length > 0
      : !!preprocessValue,
    extensions: Array.isArray(config.extensions) ? config.extensions : [],
    kitAlias: config.kit?.alias || {},
    runes: typeof config.compilerOptions?.runes === 'boolean'
      ? config.compilerOptions.runes
      : null,
    experimentalAsync: typeof config.compilerOptions?.experimental?.async === 'boolean'
      ? config.compilerOptions.experimental.async
      : null
  };
}

async function loadConfig(configPath) {
  const result = await loadEffectiveConfig(process.cwd(), configPath);
  if (result?.error) throw result.error;
  return result?.config || null;
}

function fragmentOffset(markup, phase, content) {
  if (phase === 'markup') return 0;
  const closingTag = phase === 'script' ? '</script>' : '</style>';
  const regex = phase === 'script'
    ? /<!--[^]*?-->|<script(?:\s+[^>]*?)?>([\S\s]*?)<\/script>/g
    : /<!--[^]*?-->|<style(?:\s+[^>]*?)?>([\S\s]*?)<\/style>/g;
  let offset = null;
  for (const match of markup.matchAll(regex)) {
    if (match[1] !== content) continue;
    if (offset !== null) return null;
    offset = match.index + match[0].length - closingTag.length - content.length;
  }
  return offset;
}

function annotatePreprocessorError(error, phase, args) {
  if (!error || typeof error !== 'object') return error;
  error.__svelteCheckRsPhase = phase;
  error.__svelteCheckRsFragmentOffset = fragmentOffset(args.markup || args.content, phase, args.content);
  return error;
}

function instrumentPreprocessors(value) {
  const groups = Array.isArray(value) ? value : [value];
  return groups.filter(Boolean).map((group) => {
    const instrumented = { ...group };
    for (const phase of ['markup', 'script', 'style']) {
      if (typeof group[phase] !== 'function') continue;
      instrumented[phase] = async (args) => {
        try {
          return await group[phase](args);
        } catch (error) {
          throw annotatePreprocessorError(error, phase, args);
        }
      };
    }
    return instrumented;
  });
}

function normalizePosition(position, zeroBasedLine = false) {
  if (!position || typeof position !== 'object') return null;
  const line = position.line ?? position.lineNumber ?? null;
  const column = position.column ?? position.columnNumber ?? position.col ?? null;
  const offset = position.offset ?? position.character ?? null;
  return {
    line: typeof line === 'number' ? Math.max(1, line + (zeroBasedLine ? 1 : 0)) : null,
    column: typeof column === 'number' ? Math.max(0, column) : null,
    offset: typeof offset === 'number' ? Math.max(0, offset) : null
  };
}

function normalizeFileReference(value) {
  if (value == null) return null;
  try {
    if (value instanceof URL) return value.protocol === 'file:' ? fileURLToPath(value) : value.href;
    if (typeof value === 'string' && value.startsWith('file:')) return fileURLToPath(value);
    if (typeof value === 'object' && typeof value.href === 'string') {
      return value.protocol === 'file:' || value.href.startsWith('file:')
        ? fileURLToPath(value.href)
        : value.href;
    }
  } catch {
    // Preserve the tool's original value when it is not a valid file URL.
    if (typeof value === 'object' && typeof value.href === 'string') return value.href;
  }
  return typeof value === 'string' ? value : String(value);
}

function normalizeError(error) {
  const location = error?.location || error?.loc || null;
  const spanStart = error?.span?.start || null;
  const start = location?.start || error?.start || spanStart || location || error;
  const end = location?.end || error?.end || error?.span?.end || start;
  const spanCoordinates = start === spanStart;
  const phase = error?.__svelteCheckRsPhase || null;
  const fragmentOffset = error?.__svelteCheckRsFragmentOffset ?? null;
  const hasUnmappedFragment = (phase === 'script' || phase === 'style') && fragmentOffset === null;
  return {
    message: error?.message || String(error),
    start: hasUnmappedFragment ? null : normalizePosition(start, spanCoordinates),
    end: hasUnmappedFragment ? null : normalizePosition(end, spanCoordinates),
    phase,
    fragmentOffset,
    file: normalizeFileReference(
      error?.filename || error?.file || error?.id || error?.span?.url || null
    )
  };
}

function serializeSourceMap(map) {
  if (!map) return null;
  if (typeof map === 'string') return map;
  if (map.version) return JSON.stringify(map);
  return String(map);
}

const rl = createInterface({ input: stdin, crlfDelay: Infinity });

for await (const line of rl) {
  if (!line.trim()) continue;

  let req;
  try {
    req = JSON.parse(line);
  } catch (err) {
    const message = err && err.message ? err.message : String(err);
    send({ id: null, error: `invalid json: ${message}` });
    continue;
  }

  const id = req.id;
  const filename = req.filename;
  const source = req.source;
  const options = req.options || {};

  if (req.operation === 'config') {
    try {
      const result = await loadEffectiveConfig(process.cwd(), req.configPath || null);
      send({ id, config: serializableConfig(result) });
    } catch (err) {
      send({ id, error: err?.message || String(err) });
    }
    continue;
  }

  // Compiler loading is a runner operation, not a source compilation error.
  let compiler;
  try {
    compiler = await loadCompiler(filename);
  } catch (error) {
    send({ id, error: error?.message || String(error), runnerError: true });
    continue;
  }
  if (req.operation === 'resolvecompiler') {
    send({ id, compilerIdentity: compiler.identity });
    continue;
  }

  if (req.operation === 'preprocess') {
    try {
      const config = await loadConfig(req.configPath);
      const result = config && config.preprocess
        ? await compiler.preprocess(source, instrumentPreprocessors(config.preprocess), { filename })
        : { code: source, map: null, dependencies: [] };
      send({
        id,
        code: result.code,
        map: serializeSourceMap(result.map),
        dependencies: (result.dependencies || []).map(normalizeFileReference).filter(Boolean)
      });
    } catch (err) {
      const normalized = normalizeError(err);
      send({
        id,
        error: normalized.message,
        errorStart: normalized.start,
        errorEnd: normalized.end,
        errorPhase: normalized.phase,
        errorFragmentOffset: normalized.fragmentOffset,
        errorFile: normalized.file
      });
    }
    continue;
  }

  const compileOptions = {
    filename,
    generate: options.generate || 'client',
    dev: options.dev === undefined ? true : options.dev,
    runes: options.runes
  };

  if (options.experimental != null && typeof options.experimental === 'object') {
    compileOptions.experimental = options.experimental;
  }

  let diagnostics = [];

  try {
    const result = compiler.compile(source, compileOptions);
    if (result && Array.isArray(result.warnings)) {
      diagnostics = result.warnings.map((warning) => ({
        code: warning.code || 'warning',
        message: warning.message || '',
        start: warning.start || { line: 1, column: 0 },
        end: warning.end || warning.start || { line: 1, column: 0 },
        severity: 'warning'
      }));
    }
  } catch (err) {
    const start = err && err.start ? err.start : { line: 1, column: 0 };
    const end = err && err.end ? err.end : start;
    const code = err && err.code ? err.code : 'compile_error';
    const message = err && err.message ? err.message : String(err);
    diagnostics = [{
      code,
      message,
      start,
      end,
      severity: 'error'
    }];
  }

  send({ id, diagnostics });
}

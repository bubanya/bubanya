import { useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { api, content as contentApi, modrinth, worlds as worldsApi } from "./api";
import type {
  Account,
  Accounts,
  ContentFile,
  DeviceCodeInfo,
  DownloadTask,
  FabricLoader,
  ForgeVersion,
  InstalledMod,
  Instance,
  InstanceStats,
  JavaInfo,
  LauncherUpdate,
  ModrinthVersion,
  PackManifest,
  PackUpdateInfo,
  Progress,
  Settings,
  VersionSummary,
  WorldInfo,
} from "./types";
import {
  IconArrowUpRight,
  IconCheck,
  IconChevronLeft,
  IconChip,
  IconClock,
  IconCompass,
  IconCube,
  IconDownload,
  IconFolder,
  IconGrid,
  IconHardDrive,
  IconImage,
  IconLayers,
  IconLogs,
  IconMicrosoft,
  IconMore,
  IconPackage,
  IconPause,
  IconPlay,
  IconPlus,
  IconPuzzle,
  IconRefresh,
  IconSearch,
  IconSettings,
  IconShield,
  IconSliders,
  IconSpark,
  IconTrash,
  IconUser,
  IconWorld,
  IconX,
} from "./Icons";
import logo from "./assets/bubanya-logo.png";
import "./App.css";

type View = "library" | "versions" | "browse" | "downloads" | "accounts" | "settings";
type DetailTab = "overview" | "mods" | "resourcepacks" | "shaders" | "worlds" | "logs" | "settings";
type LibraryFilter = "all" | "installed" | "fabric" | "forge" | "vanilla";
type LibrarySort = "recent" | "name" | "version" | "size";

interface BrowseItem {
  id: string;
  slug: string;
  title: string;
  description: string;
  categories: string[];
  downloads: number;
  iconUrl: string | null;
  author: string | null;
  projectType: string;
}

interface VersionChoice {
  id: string;
  versionNumber: string;
  name: string;
  gameVersions: string[];
  loaders: string[];
  size: number;
  date: string;
}

interface VersionTarget {
  projectId: string;
  title: string;
  type: "mod" | "modpack";
  instanceId: string | null;
}

const DEFAULT_SETTINGS: Settings = {
  nickname: "Player",
  ramMb: 2048,
  javaPath: null,
  closeOnLaunch: false,
  concurrency: 12,
  accent: "#E8A663",
};

const ACCENTS = [
  { name: "Bubanya", color: "#E8A663" },
  { name: "OneLite", color: "#7AA2F7" },
  { name: "Зелёный", color: "#7AD67A" },
  { name: "Фиолетовый", color: "#B48CE8" },
  { name: "Красный", color: "#E06C75" },
  { name: "Бирюзовый", color: "#64D8CB" },
];

function normalizeHex(value: string | undefined): string {
  return value && /^#[0-9a-fA-F]{6}$/.test(value) ? value : "#E8A663";
}

function hexToRgba(hex: string, alpha: number): string {
  const clean = normalizeHex(hex).slice(1);
  const red = parseInt(clean.slice(0, 2), 16);
  const green = parseInt(clean.slice(2, 4), 16);
  const blue = parseInt(clean.slice(4, 6), 16);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

function lightenHex(hex: string, amount: number): string {
  const clean = normalizeHex(hex).slice(1);
  const channels = [0, 2, 4].map((offset) => parseInt(clean.slice(offset, offset + 2), 16));
  return `#${channels
    .map((channel) => Math.round(channel + (255 - channel) * amount))
    .map((channel) => channel.toString(16).padStart(2, "0"))
    .join("")}`;
}

const DETAIL_TABS: { id: DetailTab; label: string }[] = [
  { id: "overview", label: "Обзор" },
  { id: "mods", label: "Моды" },
  { id: "resourcepacks", label: "Ресурспаки" },
  { id: "shaders", label: "Шейдеры" },
  { id: "worlds", label: "Миры" },
  { id: "logs", label: "Логи" },
  { id: "settings", label: "Настройки" },
];

function formatDate(value: string | number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "—";
  }
  return date.toLocaleDateString("ru-RU", { year: "numeric", month: "short", day: "numeric" });
}

function formatCompactDate(value: string | number): string {
  return formatDate(value).replace(" г.", "");
}

function previewLog(text: string, limit: number): string {
  return text
    .split("\n")
    .filter((line) => line.trim() && !/^\s+at\s/.test(line))
    .slice(-limit)
    .join("\n");
}

function formatRelativeDate(value: string | number): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "—";
  }
  const days = Math.floor(
    (Date.now() - date.getTime()) / (1000 * 60 * 60 * 24),
  );
  if (days <= 0) {
    return "сегодня";
  }
  if (days === 1) {
    return "вчера";
  }
  if (days < 7) {
    return `${days} дн. назад`;
  }
  return formatCompactDate(value);
}

function formatDownloads(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return String(value);
}

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)} KB`;
  }
  return `${bytes} B`;
}

function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

function instanceIconSource(icon: string | null): string | null {
  if (!icon) {
    return null;
  }
  if (/^(https?:|data:)/i.test(icon)) {
    return icon;
  }
  try {
    return convertFileSrc(icon);
  } catch {
    return null;
  }
}

function loaderLabel(loader: string): string {
  if (loader === "fabric") {
    return "Fabric";
  }
  if (loader === "forge") {
    return "Forge";
  }
  return "Vanilla";
}

function modrinthLoader(instance: Instance | null | undefined): string {
  if (!instance) {
    return "";
  }
  if (instance.loader === "fabric" || instance.loader === "forge") {
    return instance.loader;
  }
  return "";
}

function InstanceIcon({ instance, size = 22 }: { instance: Instance; size?: number }) {
  const source = instanceIconSource(instance.icon);
  if (source) {
    return <img className="instance-icon" src={source} alt="" loading="lazy" />;
  }
  const letter = instance.name.trim().slice(0, 1).toUpperCase();
  if (letter) {
    return <span className="instance-icon-letter">{letter}</span>;
  }
  return <IconCube size={size} />;
}

function applyProgress(task: DownloadTask, value: Progress): DownloadTask {
  return {
    ...task,
    stage: value.stage,
    current: value.current,
    total: value.total,
    message: value.message,
    status:
      value.stage === "done"
        ? "done"
        : task.status === "paused"
          ? "paused"
          : "active",
  };
}

function stageLabel(stage: string): string {
  switch (stage) {
    case "prepare":
      return "Манифест";
    case "client":
      return "Клиент игры";
    case "libraries":
      return "Библиотеки";
    case "assets":
      return "Ресурсы";
    case "natives":
      return "Natives";
    case "modpack":
      return "Файлы модпака";
    case "forge":
      return "Forge";
    case "done":
      return "Готово";
    default:
      return stage;
  }
}

function Tag({
  children,
  tone = "dim",
  title,
}: {
  children: ReactNode;
  tone?: "dim" | "lime" | "red";
  title?: string;
}) {
  return (
    <span className={`tag ${tone}`} title={title}>
      {children}
    </span>
  );
}

function Toggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  return (
    <button className={on ? "toggle on" : "toggle"} onClick={onClick} aria-pressed={on}>
      <span className="knob" />
    </button>
  );
}

function IconBtn({
  title,
  onClick,
  danger,
  children,
}: {
  title: string;
  onClick: () => void;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <button className={danger ? "icon-btn danger" : "icon-btn"} title={title} onClick={onClick}>
      {children}
    </button>
  );
}

function Panel({
  title,
  icon,
  actions,
  wide,
  children,
}: {
  title: string;
  icon: ReactNode;
  actions?: ReactNode;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <section className={wide ? "panel wide" : "panel"}>
      <div className="panel-head">
        <span className="panel-icon">{icon}</span>
        <h2>{title}</h2>
        {actions && <div className="panel-actions">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

export default function App() {
  const [view, setView] = useState<View>("library");
  const [detail, setDetail] = useState<Instance | null>(null);
  const [detailTab, setDetailTab] = useState<DetailTab>("overview");
  const [instanceName, setInstanceName] = useState("");
  const [instanceRam, setInstanceRam] = useState(2048);
  const [instanceRamOverride, setInstanceRamOverride] = useState(false);
  const [instanceJava, setInstanceJava] = useState("");
  const [instanceJvmArgs, setInstanceJvmArgs] = useState("");
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [versions, setVersions] = useState<VersionSummary[]>([]);
  const [java, setJava] = useState<JavaInfo[]>([]);
  const [dataDir, setDataDir] = useState("");
  const [loadingVersions, setLoadingVersions] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [booting, setBooting] = useState(true);
  const [accounts, setAccounts] = useState<Accounts>({ accounts: [], activeId: null });
  const [skins, setSkins] = useState<Record<string, string>>({});
  const [accountModal, setAccountModal] = useState<"add" | "rename" | null>(null);
  const [accountTarget, setAccountTarget] = useState<Account | null>(null);
  const [accountName, setAccountName] = useState("");
  const [accountBusy, setAccountBusy] = useState(false);
  const [javaInstalling, setJavaInstalling] = useState<number | null>(null);
  const [microsoftOpen, setMicrosoftOpen] = useState(false);
  const [microsoftCode, setMicrosoftCode] = useState<DeviceCodeInfo | null>(null);
  const [microsoftError, setMicrosoftError] = useState<string | null>(null);

  const [librarySearch, setLibrarySearch] = useState("");
  const [libraryFilter, setLibraryFilter] = useState<LibraryFilter>("all");
  const [librarySort, setLibrarySort] = useState<LibrarySort>("recent");
  const [stats, setStats] = useState<Record<string, InstanceStats>>({});
  const [cardMenu, setCardMenu] = useState<string | null>(null);
  const [renameTarget, setRenameTarget] = useState<Instance | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [working, setWorking] = useState<string | null>(null);
  const [dragActive, setDragActive] = useState(false);

  const [versionSearch, setVersionSearch] = useState("");
  const [versionFilter, setVersionFilter] = useState<"all" | "release" | "snapshot">("all");
  const [creatingVersion, setCreatingVersion] = useState<VersionSummary | null>(null);
  const [loaderKind, setLoaderKind] = useState<"vanilla" | "fabric" | "forge">("vanilla");
  const [fabricLoaders, setFabricLoaders] = useState<FabricLoader[]>([]);
  const [fabricLoader, setFabricLoader] = useState("");
  const [loadingLoaders, setLoadingLoaders] = useState(false);
  const [forgeVersions, setForgeVersions] = useState<ForgeVersion[]>([]);
  const [forgeVersion, setForgeVersion] = useState("");
  const [loadingForge, setLoadingForge] = useState(false);

  const [browseSearch, setBrowseSearch] = useState("");
  const [browseTarget, setBrowseTarget] = useState("");
  const [browseType, setBrowseType] = useState<"mod" | "modpack">("mod");
  const [browseResults, setBrowseResults] = useState<BrowseItem[]>([]);
  const [browseTotal, setBrowseTotal] = useState(0);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [browseSort, setBrowseSort] = useState("relevance");
  const [versionTarget, setVersionTarget] = useState<VersionTarget | null>(null);
  const [versionList, setVersionList] = useState<VersionChoice[]>([]);
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [installingVersion, setInstallingVersion] = useState<string | null>(null);
  const [installedMods, setInstalledMods] = useState<InstalledMod[]>([]);
  const [browseInstalled, setBrowseInstalled] = useState<InstalledMod[]>([]);
  const [modsLoading, setModsLoading] = useState(false);
  const [modSearch, setModSearch] = useState("");
  const [modFilter, setModFilter] = useState<"all" | "enabled" | "disabled">("all");
  const [dragKind, setDragKind] = useState<
    "instance" | "mod" | "modpack" | "content" | "world" | null
  >(null);
  const [packManifest, setPackManifest] = useState<PackManifest | null>(null);
  const [packManifests, setPackManifests] = useState<
    Record<string, PackManifest>
  >({});
  const [packBusy, setPackBusy] = useState(false);
  const [packUpdates, setPackUpdates] = useState<Record<string, PackUpdateInfo>>(
    {},
  );
  const [launcherUpdate, setLauncherUpdate] = useState<LauncherUpdate | null>(
    null,
  );
  const [appVersion, setAppVersion] = useState("…");
  const [logText, setLogText] = useState("");
  const [libraryLog, setLibraryLog] = useState("");
  const [logLoading, setLogLoading] = useState(false);
  const detailRef = useRef<Instance | null>(null);
  const detailTabRef = useRef<DetailTab>("overview");
  const searchRef = useRef<HTMLInputElement | null>(null);
  const [resourcepacks, setResourcepacks] = useState<ContentFile[]>([]);
  const [shaders, setShaders] = useState<ContentFile[]>([]);
  const [worlds, setWorlds] = useState<WorldInfo[]>([]);
  const [downloads, setDownloads] = useState<DownloadTask[]>([]);

  useEffect(() => {
    void (async () => {
      try {
        const [loadedSettings, loadedInstances, loadedJava, dir, loadedAccounts] =
          await Promise.all([
            api.getSettings(),
            api.listInstances(),
            api.detectJava(),
            api.getDataDir(),
            api.listAccounts(),
          ]);
        setSettings(loadedSettings);
        setInstances(loadedInstances);
        setJava(loadedJava);
        setDataDir(dir);
        setAccounts(loadedAccounts);
      } catch (cause) {
        setError(String(cause));
      }
    })();
  }, []);

  useEffect(() => {
    const microsoftAccounts = accounts.accounts.filter(
      (account) => account.kind === "microsoft",
    );
    if (microsoftAccounts.length === 0) {
      setSkins({});
      return;
    }
    let cancelled = false;
    void Promise.all(
      microsoftAccounts.map(async (account) => {
        try {
          const url = await api.getAccountSkin(account.uuid);
          return [account.uuid, url] as const;
        } catch {
          return [account.uuid, null] as const;
        }
      }),
    ).then((entries) => {
      if (cancelled) {
        return;
      }
      const next: Record<string, string> = {};
      for (const [uuid, url] of entries) {
        if (url) {
          next[uuid] = url;
        }
      }
      setSkins(next);
    });
    return () => {
      cancelled = true;
    };
  }, [accounts]);

  useEffect(() => {
    let disposed = false;
    let dispose: (() => void) | undefined;
    void api
      .onProgress((value) => {
        setDownloads((list) => {
          if (list.some((item) => item.id === value.instanceId)) {
            return list.map((item) =>
              item.id === value.instanceId ? applyProgress(item, value) : item,
            );
          }
          const modpack = list.find(
            (item) =>
              item.id.startsWith("modpack:") &&
              (item.status === "active" || item.status === "paused"),
          );
          if (modpack) {
            return list.map((item) =>
              item.id === modpack.id ? applyProgress(item, value) : item,
            );
          }
          return list;
        });
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          dispose = unlisten;
        }
      });
    return () => {
      disposed = true;
      dispose?.();
    };
  }, []);

  useEffect(() => {
    if (!toast) {
      return;
    }
    const timer = window.setTimeout(() => setToast(null), 2600);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    const timer = window.setTimeout(() => setBooting(false), 700);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    detailRef.current = detail;
  }, [detail]);

  useEffect(() => {
    detailTabRef.current = detailTab;
  }, [detailTab]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!detail) {
      return;
    }
    setInstanceName(detail.name);
    setInstanceRam(detail.ramMb ?? settings.ramMb);
    setInstanceRamOverride(detail.ramMb !== null);
    setInstanceJava(detail.javaPath ?? "");
    setInstanceJvmArgs(detail.jvmArgs ?? "");
  }, [detail?.id, settings.ramMb]);

  useEffect(() => {
    let cancelled = false;
    void api
      .listPackManifests()
      .then((manifests) => {
        if (!cancelled) {
          setPackManifests(manifests);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [instances]);

  // Обновления модпаков: фоновая проверка раз в сутки (кэш на стороне Rust),
  // при каждом изменении списка инстансов просто перечитываем актуальное состояние.
  useEffect(() => {
    if (instances.length === 0) {
      setPackUpdates({});
      return;
    }
    let cancelled = false;
    void api
      .checkPackUpdates()
      .then((list) => {
        if (cancelled) {
          return;
        }
        const map: Record<string, PackUpdateInfo> = {};
        for (const info of list) {
          map[info.instanceId] = info;
        }
        setPackUpdates(map);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [instances]);

  // Новая версия лаунчера: раз в сутки, тихо, без сети баннер не показываем.
  useEffect(() => {
    let cancelled = false;
    void api
      .checkLauncherUpdate()
      .then((update) => {
        if (!cancelled) {
          setLauncherUpdate(update);
        }
      })
      .catch(() => {});
    void getVersion()
      .then((version) => {
        if (!cancelled && version) {
          setAppVersion(version);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!detail) {
      setPackManifest(null);
      return;
    }
    let cancelled = false;
    void api
      .getPackManifest(detail.id)
      .then((manifest) => {
        if (!cancelled) {
          setPackManifest(manifest);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPackManifest(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [detail?.id]);

  useEffect(() => {
    if (!detail) {
      setLogText("");
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const text = await api.readLog(detail.id, 800);
        if (!cancelled) {
          setLogText(text);
        }
      } catch {
        // лог может быть ещё не создан
      }
    };
    void load();
    const timer = window.setInterval(load, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [detail?.id]);

  useEffect(() => {
    if (view !== "library") {
      return;
    }
    if (instances.length === 0) {
      setStats({});
      return;
    }
    let cancelled = false;
    void (async () => {
      const entries = await Promise.all(
        instances.map(async (instance) => {
          try {
            return [instance.id, await api.instanceStats(instance.id)] as const;
          } catch {
            return null;
          }
        }),
      );
      if (cancelled) {
        return;
      }
      setStats((current) => {
        const next = { ...current };
        const ids = new Set(instances.map((instance) => instance.id));
        for (const id of Object.keys(next)) {
          if (!ids.has(id)) {
            delete next[id];
          }
        }
        for (const entry of entries) {
          if (entry) {
            next[entry[0]] = entry[1];
          }
        }
        return next;
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [view, instances]);

  useEffect(() => {
    let disposed = false;
    let dispose: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter") {
          const jars = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".jar"),
          );
          const zips = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".zip"),
          );
          const packs = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".mrpack"),
          );
          const tab = detailTabRef.current;
          setDragKind(
            packs.length > 0
              ? "modpack"
              : jars.length > 0 &&
                  zips.length === 0 &&
                  detailRef.current !== null &&
                  tab === "mods"
                ? "mod"
                : zips.length > 0 &&
                    detailRef.current !== null &&
                    (tab === "resourcepacks" || tab === "shaders")
                  ? "content"
                  : zips.length > 0 &&
                      detailRef.current !== null &&
                      tab === "worlds"
                    ? "world"
                    : "instance",
          );
          setDragActive(true);
        } else if (event.payload.type === "over") {
          setDragActive(true);
        } else if (event.payload.type === "leave") {
          setDragActive(false);
          setDragKind(null);
        } else if (event.payload.type === "drop") {
          setDragActive(false);
          setDragKind(null);
          const jars = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".jar"),
          );
          const archives = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".zip"),
          );
          const packs = event.payload.paths.filter((path) =>
            path.toLowerCase().endsWith(".mrpack"),
          );
          const tab = detailTabRef.current;
          if (packs.length > 0) {
            void importModpacks(packs);
          } else if (jars.length > 0 && detailRef.current && tab === "mods") {
            void installModPaths(detailRef.current.id, jars);
          } else if (
            archives.length > 0 &&
            detailRef.current &&
            (tab === "resourcepacks" || tab === "shaders")
          ) {
            void importContentPaths(
              tab === "resourcepacks" ? "resourcepacks" : "shaderpacks",
              archives,
            );
          } else if (archives.length > 0 && detailRef.current && tab === "worlds") {
            void importWorldPaths(archives);
          } else if (archives.length > 0) {
            void importArchives(archives);
          } else if (jars.length > 0) {
            setToast("Открой инстанс → Моды, чтобы установить .jar");
          } else {
            setToast("Нужен .zip, .mrpack или .jar");
          }
        }
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          dispose = unlisten;
        }
      });
    return () => {
      disposed = true;
      dispose?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!microsoftOpen || !microsoftCode) {
      return;
    }
    let cancelled = false;
    let timer: number | undefined;
    const interval = Math.max(3, microsoftCode.interval || 5) * 1000;
    const poll = async () => {
      try {
        const result = await api.microsoftPollLogin(microsoftCode.deviceCode);
        if (cancelled) {
          return;
        }
        if (result.status === "success" && result.accounts) {
          applyAccounts(result.accounts);
          setMicrosoftOpen(false);
          setMicrosoftCode(null);
          setToast("Microsoft подключён");
          return;
        }
        if (result.status === "error") {
          setMicrosoftError(result.message);
          return;
        }
        timer = window.setTimeout(poll, interval);
      } catch (cause) {
        if (!cancelled) {
          setMicrosoftError(String(cause));
        }
      }
    };
    timer = window.setTimeout(poll, interval);
    return () => {
      cancelled = true;
      if (timer) {
        window.clearTimeout(timer);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [microsoftOpen, microsoftCode]);

  const filteredVersions = useMemo(() => {
    const query = versionSearch.trim().toLowerCase();
    return versions
      .filter((version) => !query || version.id.toLowerCase().includes(query))
      .filter((version) => versionFilter === "all" || version.kind === versionFilter)
      .slice(0, 400);
  }, [versions, versionSearch, versionFilter]);

  const filteredInstances = useMemo(() => {
    const query = librarySearch.trim().toLowerCase();
    const result = instances
      .filter((instance) => !query || instance.name.toLowerCase().includes(query))
      .filter((instance) => {
        if (libraryFilter === "installed") return instance.installed;
        if (libraryFilter === "fabric") return instance.loader === "fabric";
        if (libraryFilter === "forge") return instance.loader === "forge";
        if (libraryFilter === "vanilla") {
          return instance.loader !== "fabric" && instance.loader !== "forge";
        }
        return true;
      });
    result.sort((a, b) => {
      if (librarySort === "name") return a.name.localeCompare(b.name);
      if (librarySort === "version") return a.versionId.localeCompare(b.versionId);
      if (librarySort === "size") {
        return (stats[b.id]?.sizeBytes ?? 0) - (stats[a.id]?.sizeBytes ?? 0);
      }
      return (b.lastPlayed ?? b.createdAt) - (a.lastPlayed ?? a.createdAt);
    });
    return result;
  }, [instances, librarySearch, libraryFilter, librarySort, stats]);

  const filteredMods = useMemo(() => {
    const query = modSearch.trim().toLowerCase();
    return installedMods.filter((mod) => {
      if (modFilter === "enabled" && !mod.enabled) {
        return false;
      }
      if (modFilter === "disabled" && mod.enabled) {
        return false;
      }
      if (!query) {
        return true;
      }
      return (
        mod.name.toLowerCase().includes(query) ||
        mod.filename.toLowerCase().includes(query)
      );
    });
  }, [installedMods, modSearch, modFilter]);

  const enabledMods = installedMods.filter((mod) => mod.enabled).length;

  const quickPlay = useMemo(() => {
    const installed = instances.filter((instance) => instance.installed);
    installed.sort((a, b) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0));
    return installed[0] ?? null;
  }, [instances]);

  useEffect(() => {
    if (!quickPlay) {
      setLibraryLog("");
      return;
    }
    let cancelled = false;
    void api
      .readLog(quickPlay.id, 80)
      .then((text) => {
        if (!cancelled) {
          setLibraryLog(text);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [quickPlay?.id, quickPlay?.lastPlayed]);

  const activeAccount =
    accounts.accounts.find((account) => account.id === accounts.activeId) ??
    accounts.accounts[0] ??
    null;
  const skinFor = (account: Account | null | undefined) =>
    account?.kind === "microsoft" ? skins[account.uuid] ?? null : null;
  const installedCount = instances.filter((instance) => instance.installed).length;
  const totalStats = useMemo(() => {
    const values = Object.values(stats);
    return {
      size: values.reduce((sum, item) => sum + item.sizeBytes, 0),
      mods: values.reduce((sum, item) => sum + item.mods, 0),
      worlds: values.reduce((sum, item) => sum + item.worlds, 0),
    };
  }, [stats]);
  const activeDownloads = downloads.filter(
    (item) => item.status === "active" || item.status === "paused",
  ).length;
  const activeTask =
    downloads.find((item) => item.status === "active" || item.status === "paused") ?? null;
  const targetInstance =
    instances.find((instance) => instance.id === browseTarget) ??
    quickPlay ??
    instances[0] ??
    null;

  useEffect(() => {
    if (!browseTarget && instances.length > 0) {
      setBrowseTarget(quickPlay?.id ?? instances[0].id);
    }
  }, [instances, quickPlay, browseTarget]);

  useEffect(() => {
    if (view === "versions" && versions.length === 0 && !loadingVersions) {
      void loadVersions();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  useEffect(() => {
    if (view === "browse" && (targetInstance || browseType === "modpack")) {
      void loadBrowse(0, false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, browseTarget, browseType, browseSort]);

  useEffect(() => {
    if (targetInstance) {
      void loadBrowseInstalled(targetInstance.id);
    } else {
      setBrowseInstalled([]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetInstance?.id]);

  function applyAccounts(result: Accounts) {
    setAccounts(result);
    const active = result.accounts.find((account) => account.id === result.activeId);
    if (active) {
      setSettings((current) =>
        current.nickname === active.name ? current : { ...current, nickname: active.name },
      );
    }
  }

  function openAddAccount() {
    setAccountTarget(null);
    setAccountName("");
    setAccountModal("add");
  }

  function closeMicrosoft() {
    setMicrosoftOpen(false);
    setMicrosoftCode(null);
    setMicrosoftError(null);
  }

  async function openMicrosoft() {
    setMicrosoftOpen(true);
    setMicrosoftCode(null);
    setMicrosoftError(null);
    try {
      setMicrosoftCode(await api.microsoftBeginLogin());
    } catch (cause) {
      setMicrosoftError(String(cause));
    }
  }

  function openRenameAccount(account: Account) {
    setAccountTarget(account);
    setAccountName(account.name);
    setAccountModal("rename");
  }

  async function saveAccount() {
    const name = accountName.trim();
    if (!name) {
      setError("Введи ник");
      return;
    }
    setAccountBusy(true);
    setError(null);
    try {
      if (accountModal === "add") {
        applyAccounts(await api.addOfflineAccount(name));
        setToast("Аккаунт добавлен");
      } else if (accountModal === "rename" && accountTarget) {
        applyAccounts(await api.renameAccount(accountTarget.id, name));
        setToast("Аккаунт переименован");
      }
      setAccountModal(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setAccountBusy(false);
    }
  }

  async function selectAccount(account: Account) {
    setError(null);
    try {
      applyAccounts(await api.setActiveAccount(account.id));
      setToast(`Аккаунт: ${account.name}`);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function removeAccount(account: Account) {
    if (!window.confirm(`Удалить аккаунт «${account.name}»?`)) {
      return;
    }
    setError(null);
    try {
      applyAccounts(await api.deleteAccount(account.id));
      setToast("Аккаунт удалён");
    } catch (cause) {
      setError(String(cause));
    }
  }

  function startDownload(id: string, name: string, version: string) {
    setDownloads((list) => [
      {
        id,
        name,
        version,
        stage: "prepare",
        current: 0,
        total: 0,
        status: "active",
      },
      ...list.filter((item) => item.id !== id),
    ]);
  }

  function finishDownload(id: string, error?: string) {
    setDownloads((list) =>
      list.map((item) =>
        item.id === id
          ? { ...item, status: error ? "error" : "done", message: error ?? item.message }
          : item,
      ),
    );
  }

  async function loadBrowse(offset = 0, append = false) {
    if (browseType === "mod" && !targetInstance) {
      setBrowseResults([]);
      setBrowseTotal(0);
      return;
    }
    setBrowseLoading(true);
    setError(null);
    try {
      const gameVersion =
        browseType === "mod" && targetInstance ? targetInstance.versionId : "";
      const loader = browseType === "mod" ? modrinthLoader(targetInstance) : "";
      const result = await modrinth.search(
        browseSearch,
        gameVersion,
        loader,
        browseType,
        browseSort,
        offset,
        20,
      );
      const items: BrowseItem[] = result.hits.map((hit) => ({
        id: hit.projectId,
        slug: hit.slug,
        title: hit.title,
        description: hit.description,
        categories: hit.categories,
        downloads: hit.downloads,
        iconUrl: hit.iconUrl,
        author: hit.author,
        projectType: hit.projectType,
      }));
      setBrowseResults((list) => (append ? [...list, ...items] : items));
      setBrowseTotal(result.total);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBrowseLoading(false);
    }
  }

  function mapModrinthVersions(versions: ModrinthVersion[]): VersionChoice[] {
    return versions.map((version) => ({
      id: version.id,
      versionNumber: version.versionNumber,
      name: version.name,
      gameVersions: version.gameVersions,
      loaders: version.loaders,
      size: version.size,
      date: version.datePublished,
    }));
  }

  async function openVersions(item: BrowseItem) {
    const type = item.projectType === "modpack" ? "modpack" : "mod";
    setVersionTarget({
      projectId: item.id,
      title: item.title,
      type,
      instanceId: type === "mod" ? targetInstance?.id ?? null : null,
    });
    setVersionList([]);
    setVersionsLoading(true);
    setError(null);
    try {
      const gameVersion = type === "mod" && targetInstance ? targetInstance.versionId : "";
      const loader = type === "mod" ? modrinthLoader(targetInstance) : "";
      setVersionList(
        mapModrinthVersions(await modrinth.versions(item.id, gameVersion, loader)),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setVersionsLoading(false);
    }
  }

  async function openModVersions(mod: InstalledMod) {
    if (!mod.projectId || !detail) {
      setToast("У мода нет project_id — сначала переиндексируй");
      return;
    }
    if (mod.projectId.startsWith("curseforge:")) {
      setToast("CurseForge больше не поддерживается — удали этот мод");
      return;
    }
    setVersionTarget({
      projectId: mod.projectId,
      title: mod.name,
      type: "mod",
      instanceId: detail.id,
    });
    setVersionList([]);
    setVersionsLoading(true);
    setError(null);
    try {
      setVersionList(
        mapModrinthVersions(
          await modrinth.versions(
            mod.projectId,
            detail.versionId,
            modrinthLoader(detail),
          ),
        ),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setVersionsLoading(false);
    }
  }

  async function installChosenVersion(version: VersionChoice) {
    if (!versionTarget) {
      return;
    }
    const modpackTaskId = `modpack:${versionTarget.projectId}`;
    setInstallingVersion(version.id);
    setError(null);
    try {
      if (versionTarget.type === "modpack") {
        startDownload(modpackTaskId, versionTarget.title, version.versionNumber);
        const instance = await modrinth.installModpackVersion(
          versionTarget.projectId,
          version.id,
        );
        setInstances((list) => [instance, ...list]);
        finishDownload(modpackTaskId);
        setToast(`Модпак «${instance.name}» установлен`);
        setVersionTarget(null);
        setView("library");
      } else {
        const instanceId = versionTarget.instanceId;
        if (!instanceId) {
          setToast("Сначала создай инстанс");
          return;
        }
        const installed = await modrinth.installVersion(instanceId, version.id);
        setToast(`Установлен ${installed.name} ${installed.version}`);
        await loadBrowseInstalled(instanceId);
        if (detail?.id === instanceId) {
          await loadInstalledMods(instanceId);
        }
        setVersionTarget(null);
      }
    } catch (cause) {
      setError(String(cause));
      if (versionTarget.type === "modpack") {
        finishDownload(modpackTaskId, String(cause));
      }
    } finally {
      setInstallingVersion(null);
    }
  }

  async function loadInstalledMods(instanceId: string) {
    setModsLoading(true);
    try {
      setInstalledMods(await modrinth.listMods(instanceId));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setModsLoading(false);
    }
  }

  async function loadContent(instanceId: string, kind: "resourcepacks" | "shaderpacks") {
    try {
      const items = await contentApi.list(instanceId, kind);
      if (kind === "resourcepacks") {
        setResourcepacks(items);
      } else {
        setShaders(items);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function loadWorlds(instanceId: string) {
    try {
      setWorlds(await worldsApi.list(instanceId));
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function toggleContentItem(
    item: ContentFile,
    kind: "resourcepacks" | "shaderpacks",
  ) {
    if (!detail) {
      return;
    }
    try {
      await contentApi.toggle(detail.id, kind, item.filename);
      await loadContent(detail.id, kind);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function deleteContentItem(
    item: ContentFile,
    kind: "resourcepacks" | "shaderpacks",
  ) {
    if (!detail) {
      return;
    }
    if (!window.confirm(`Удалить «${item.name}»?`)) {
      return;
    }
    try {
      await contentApi.remove(detail.id, kind, item.filename);
      await loadContent(detail.id, kind);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function importContent(kind: "resourcepacks" | "shaderpacks") {
    if (!detail) {
      return;
    }
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Minecraft pack", extensions: ["zip"] }],
      });
      if (typeof selected === "string") {
        await contentApi.import(detail.id, kind, selected);
        await loadContent(detail.id, kind);
        setToast(kind === "resourcepacks" ? "Ресурспак добавлен" : "Шейдер добавлен");
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function importContentPaths(
    kind: "resourcepacks" | "shaderpacks",
    paths: string[],
  ) {
    if (!detail) {
      return;
    }
    let added = 0;
    for (const path of paths) {
      try {
        await contentApi.import(detail.id, kind, path);
        added += 1;
      } catch (cause) {
        setError(String(cause));
      }
    }
    if (added > 0) {
      await loadContent(detail.id, kind);
      setToast(
        kind === "resourcepacks" ? "Ресурспак добавлен" : "Шейдер добавлен",
      );
    }
  }

  async function importWorldPaths(paths: string[]) {
    if (!detail) {
      return;
    }
    let added = 0;
    for (const path of paths) {
      try {
        await worldsApi.import(detail.id, path);
        added += 1;
      } catch (cause) {
        setError(String(cause));
      }
    }
    if (added > 0) {
      await loadWorlds(detail.id);
      setToast("Мир импортирован");
    }
  }

  async function openContentDir(kind: "resourcepacks" | "shaderpacks") {
    if (!detail) {
      return;
    }
    try {
      await openPath(await contentApi.dir(detail.id, kind));
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function deleteWorldItem(world: WorldInfo) {
    if (!detail) {
      return;
    }
    if (!window.confirm(`Удалить мир «${world.name}»? Это необратимо.`)) {
      return;
    }
    try {
      await worldsApi.remove(detail.id, world.folder);
      await loadWorlds(detail.id);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function backupWorldItem(world: WorldInfo) {
    if (!detail) {
      return;
    }
    try {
      const path = await worldsApi.backup(detail.id, world.folder);
      setToast(`Бэкап: ${fileName(path)}`);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function importWorldItem() {
    if (!detail) {
      return;
    }
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Minecraft world", extensions: ["zip"] }],
      });
      if (typeof selected === "string") {
        await worldsApi.import(detail.id, selected);
        await loadWorlds(detail.id);
        setToast("Мир импортирован");
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openWorldsDir() {
    if (!detail) {
      return;
    }
    try {
      await openPath(await worldsApi.dir(detail.id));
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function loadBrowseInstalled(instanceId: string) {
    try {
      setBrowseInstalled(await modrinth.listMods(instanceId));
    } catch {
      setBrowseInstalled([]);
    }
  }

  function isInstalled(item: BrowseItem): boolean {
    return browseInstalled.some((mod) => {
      if (mod.projectId === item.id) {
        return true;
      }
      const base = mod.filename
        .toLowerCase()
        .replace(/\.jar(\.disabled)?$/, "")
        .replace(/_/g, "-");
      const slug = item.slug.toLowerCase();
      const title = item.title.toLowerCase().replace(/\s+/g, "-");
      return base === slug || base === title;
    });
  }

  async function toggleInstalledMod(mod: InstalledMod) {
    if (!detail) {
      return;
    }
    try {
      await modrinth.toggleMod(detail.id, mod.filename);
      await loadInstalledMods(detail.id);
      if (targetInstance?.id === detail.id) {
        await loadBrowseInstalled(detail.id);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function deleteInstalledMod(mod: InstalledMod) {
    if (!detail) {
      return;
    }
    if (!window.confirm(`Удалить мод «${mod.name}»?`)) {
      return;
    }
    try {
      await modrinth.deleteMod(detail.id, mod.filename);
      await loadInstalledMods(detail.id);
      if (targetInstance?.id === detail.id) {
        await loadBrowseInstalled(detail.id);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function installModPaths(instanceId: string, paths: string[]) {
    setModsLoading(true);
    setError(null);
    let installed = 0;
    for (const path of paths) {
      try {
        await modrinth.installModFile(instanceId, path);
        installed += 1;
      } catch (cause) {
        setError(String(cause));
      }
    }
    await loadInstalledMods(instanceId);
    await loadBrowseInstalled(instanceId);
    setModsLoading(false);
    if (installed > 0) {
      setToast(installed === 1 ? "Мод установлен" : `Установлено модов: ${installed}`);
    }
  }

  async function installModFromFile() {
    if (!detail) {
      return;
    }
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Minecraft mod", extensions: ["jar"] }],
      });
      if (typeof selected === "string") {
        await installModPaths(detail.id, [selected]);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function updateMods(instance: Instance) {
    setModsLoading(true);
    setError(null);
    try {
      const updated = await modrinth.updateAll(
        instance.id,
        instance.versionId,
        modrinthLoader(instance),
      );
      await loadInstalledMods(instance.id);
      if (targetInstance?.id === instance.id) {
        await loadBrowseInstalled(instance.id);
      }
      setToast(updated.length > 0 ? `Обновлено модов: ${updated.length}` : "Обновлений нет");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setModsLoading(false);
    }
  }

  async function reindexMods(instance: Instance) {
    setModsLoading(true);
    setError(null);
    try {
      const mods = await modrinth.reindex(instance.id);
      setInstalledMods(mods);
      if (targetInstance?.id === instance.id) {
        await loadBrowseInstalled(instance.id);
      }
      setToast(`Проиндексировано модов: ${mods.length}`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setModsLoading(false);
    }
  }

  function openDetail(instance: Instance) {
    setDetail(instance);
    setDetailTab("overview");
    setModSearch("");
    setModFilter("all");
    void loadInstalledMods(instance.id);
    void loadContent(instance.id, "resourcepacks");
    void loadContent(instance.id, "shaderpacks");
    void loadWorlds(instance.id);
  }

  function instanceMenu(instance: Instance) {
    return (
      <div className="menu-wrap">
        <IconBtn
          title="Ещё"
          onClick={() =>
            setCardMenu(cardMenu === instance.id ? null : instance.id)
          }
        >
          <IconMore size={16} />
        </IconBtn>
        {cardMenu === instance.id && (
          <>
            <div className="menu-backdrop" onClick={() => setCardMenu(null)} />
            <div className="card-menu" onClick={(event) => event.stopPropagation()}>
              <button className="menu-item" onClick={() => startRename(instance)}>
                Переименовать
              </button>
              <button
                className="menu-item"
                disabled={working === instance.id}
                onClick={() => duplicate(instance)}
              >
                Дублировать
              </button>
              <button
                className="menu-item"
                disabled={working === instance.id}
                onClick={() => exportInstance(instance)}
                title="Полный экспорт: instance.json + game/"
              >
                {working === instance.id ? "Экспорт…" : "Экспорт инстанса (.zip)"}
              </button>
              <button
                className="menu-item"
                disabled={working === instance.id}
                onClick={() => exportGame(instance)}
                title="mods, config, resourcepacks и т.д. Без миров, логов и скриншотов"
              >
                Экспорт сборки (.zip)
              </button>
              <div className="menu-sep" />
              <button className="menu-item danger" onClick={() => remove(instance)}>
                Удалить
              </button>
            </div>
          </>
        )}
      </div>
    );
  }

  async function loadVersions() {
    setLoadingVersions(true);
    setError(null);
    try {
      setVersions(await api.fetchVersions());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoadingVersions(false);
    }
  }

  async function saveSettings() {
    setError(null);
    try {
      await api.saveSettings(settings);
      setToast("Настройки сохранены");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function saveInstanceSettings() {
    if (!detail) {
      return;
    }
    setError(null);
    try {
      const updated = await api.updateInstanceSettings(
        detail.id,
        instanceName,
        instanceRamOverride ? instanceRam : null,
        instanceJava || null,
        instanceJvmArgs || null,
      );
      setInstances((list) =>
        list.map((item) => (item.id === updated.id ? updated : item)),
      );
      setDetail(updated);
      setToast("Настройки инстанса сохранены");
    } catch (cause) {
      setError(String(cause));
    }
  }

  function hasManagedJava(major: number): boolean {
    return java.some((item) =>
      item.path
        .replace(/\\/g, "/")
        .includes(`/Bubanya/java/${major}/`),
    );
  }

  async function installJava(major: number) {
    setJavaInstalling(major);
    setError(null);
    setToast(`Скачиваю Java ${major}…`);
    try {
      const info = await api.installJava(major);
      setJava(await api.detectJava());
      setToast(`Java ${info.major} установлена`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setJavaInstalling(null);
    }
  }

  async function removeJava(major: number) {
    if (!window.confirm(`Удалить скачанную Java ${major}?`)) {
      return;
    }
    setError(null);
    try {
      await api.removeJava(major);
      setJava(await api.detectJava());
      setToast(`Java ${major} удалена`);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function cancelDownload(id: string) {
    try {
      await api.cancelDownload(id);
      setToast("Отмена загрузки…");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function pauseDownload(id: string) {
    try {
      await api.pauseDownload(id);
      setDownloads((list) =>
        list.map((item) =>
          item.id === id ? { ...item, status: "paused" } : item,
        ),
      );
      setToast("Загрузка на паузе");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function resumeDownload(id: string) {
    try {
      await api.resumeDownload(id);
      setDownloads((list) =>
        list.map((item) =>
          item.id === id ? { ...item, status: "active" } : item,
        ),
      );
      setToast("Продолжаю загрузку");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function refreshLog() {
    if (!detail) {
      return;
    }
    setLogLoading(true);
    try {
      setLogText(await api.readLog(detail.id, 2000));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLogLoading(false);
    }
  }

  async function importModpacks(paths: string[]) {
    setError(null);
    setToast("Импортирую модпак…");
    let imported = 0;
    for (const [index, path] of paths.entries()) {
      const name = path.split(/[\\/]/).pop() ?? "Модпак";
      const taskId = `modpack:import:${Date.now()}:${index}`;
      startDownload(taskId, name, "mrpack");
      try {
        const instance = await api.importModpackFile(path, taskId);
        setInstances((list) => [instance, ...list]);
        finishDownload(taskId);
        imported += 1;
      } catch (cause) {
        finishDownload(taskId, String(cause));
        setError(String(cause));
      }
    }
    if (imported > 0) {
      setView("library");
      setDetail(null);
      setToast("Модпак импортирован");
    }
  }

  async function pickModpack() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Modrinth modpack", extensions: ["mrpack"] }],
      });
      if (typeof selected === "string") {
        await importModpacks([selected]);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function updatePack() {
    if (!detail) {
      return;
    }
    setPackBusy(true);
    setError(null);
    setToast("Обновляю модпак…");
    startDownload(detail.id, detail.name, "обновление");
    try {
      const updated = await api.updateModpack(detail.id, null);
      setInstances((list) =>
        list.map((item) => (item.id === updated.id ? updated : item)),
      );
      setDetail(updated);
      setPackManifest(await api.getPackManifest(updated.id));
      finishDownload(detail.id);
      setToast("Модпак обновлён");
    } catch (cause) {
      finishDownload(detail.id, String(cause));
      setError(String(cause));
    } finally {
      setPackBusy(false);
    }
  }

  async function dismissUpdateBanner() {
    setLauncherUpdate((current) =>
      current ? { ...current, dismissed: true } : current,
    );
    try {
      await api.dismissLauncherUpdate();
    } catch {
      // кэш недоступен — просто скрываем до перезапуска
    }
  }

  async function createAndInstall(versionId: string, loader: string, loaderVersion: string | null) {
    setBusy(versionId);
    setError(null);
    let instanceId: string | null = null;
    try {
      const instance = await api.createInstance(versionId, versionId, loader, loaderVersion);
      instanceId = instance.id;
      setInstances((list) => [instance, ...list]);
      setView("library");
      startDownload(instance.id, instance.name, instance.versionId);
      const installed = await api.installInstance(instance.id);
      setInstances((list) => list.map((item) => (item.id === installed.id ? installed : item)));
      finishDownload(instance.id);
      setToast(`Инстанс ${installed.name} готов`);
    } catch (cause) {
      setError(String(cause));
      if (instanceId) {
        finishDownload(instanceId, String(cause));
      }
    } finally {
      setBusy(null);
    }
  }

  async function openCreate(version: VersionSummary) {
    setCreatingVersion(version);
    setLoaderKind("vanilla");
    setFabricLoaders([]);
    setFabricLoader("");
    setForgeVersions([]);
    setForgeVersion("");
    setLoadingLoaders(true);
    setLoadingForge(true);
    try {
      const loaders = await api.fetchFabricLoaders(version.id);
      setFabricLoaders(loaders);
      const stable = loaders.find((item) => item.stable) ?? loaders[0];
      setFabricLoader(stable?.version ?? "");
    } catch {
      setFabricLoaders([]);
    } finally {
      setLoadingLoaders(false);
    }
    try {
      const versions = await api.fetchForgeVersions(version.id);
      setForgeVersions(versions);
      const recommended = versions.find((item) => item.recommended) ?? versions[0];
      setForgeVersion(recommended?.version ?? "");
    } catch {
      setForgeVersions([]);
    } finally {
      setLoadingForge(false);
    }
  }

  async function confirmCreate() {
    if (!creatingVersion) {
      return;
    }
    const version = creatingVersion;
    const loader = loaderKind;
    const loaderVersion =
      loader === "fabric" ? fabricLoader : loader === "forge" ? forgeVersion : null;
    setCreatingVersion(null);
    await createAndInstall(version.id, loader, loaderVersion);
  }

  async function install(instance: Instance) {
    setBusy(instance.id);
    setError(null);
    startDownload(instance.id, instance.name, instance.versionId);
    try {
      const installed = await api.installInstance(instance.id);
      setInstances((list) => list.map((item) => (item.id === installed.id ? installed : item)));
      finishDownload(instance.id);
    } catch (cause) {
      setError(String(cause));
      finishDownload(instance.id, String(cause));
    } finally {
      setBusy(null);
    }
  }

  async function play(instance: Instance) {
    setBusy(instance.id);
    setError(null);
    try {
      const check = await api.checkInstanceJava(instance.id);
      if (!check.found && check.required > 0) {
        setToast(`Java ${check.required} не найдена — скачиваю…`);
        await api.installJava(check.required);
        setJava(await api.detectJava());
        setToast(`Java ${check.required} установлена`);
      }
      await api.launchInstance(instance.id);
      setToast(`Запуск ${instance.name}`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  }

  async function remove(instance: Instance) {
    if (!window.confirm(`Удалить инстанс «${instance.name}»?`)) {
      return;
    }
    setError(null);
    try {
      await api.deleteInstance(instance.id);
      setInstances((list) => list.filter((item) => item.id !== instance.id));
      if (detail?.id === instance.id) {
        setDetail(null);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  function openPackInstance(instanceId: string) {
    const instance = instances.find((item) => item.id === instanceId);
    if (!instance) {
      return;
    }
    setDetail(instance);
    setDetailTab("overview");
    setView("library");
  }

  async function openFolder(instance: Instance) {
    try {
      await openPath(await api.getInstanceDir(instance.id));
    } catch (cause) {
      setError(String(cause));
    }
  }

  function startRename(instance: Instance) {
    setCardMenu(null);
    setRenameTarget(instance);
    setRenameValue(instance.name);
  }

  async function confirmRename() {
    if (!renameTarget) {
      return;
    }
    const name = renameValue.trim();
    if (!name) {
      setError("Имя не может быть пустым");
      return;
    }
    setError(null);
    try {
      const updated = await api.renameInstance(renameTarget.id, name);
      setInstances((list) => list.map((item) => (item.id === updated.id ? updated : item)));
      setDetail((current) => (current?.id === updated.id ? updated : current));
      setRenameTarget(null);
      setToast("Инстанс переименован");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function duplicate(instance: Instance) {
    setCardMenu(null);
    setWorking(instance.id);
    setError(null);
    setToast("Дублирую инстанс…");
    try {
      const copy = await api.duplicateInstance(instance.id);
      setInstances((list) => [copy, ...list]);
      setToast(`Создана копия «${copy.name}»`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWorking(null);
    }
  }

  async function exportInstance(instance: Instance) {
    setCardMenu(null);
    setWorking(instance.id);
    setError(null);
    setToast("Экспортирую инстанс…");
    try {
      const path = await api.exportInstance(instance.id);
      setToast(`Экспортировано: ${fileName(path)}`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWorking(null);
    }
  }

  async function exportGame(instance: Instance) {
    setCardMenu(null);
    setWorking(instance.id);
    setError(null);
    setToast("Экспортирую сборку…");
    try {
      const path = await api.exportInstanceGame(instance.id);
      setToast(`Сборка: ${fileName(path)}`);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setWorking(null);
    }
  }

  async function importArchives(paths: string[]) {
    setError(null);
    setToast(paths.length === 1 ? "Импортирую инстанс…" : "Импортирую инстансы…");
    let imported = 0;
    for (const path of paths) {
      try {
        const instance = await api.importInstance(path);
        setInstances((list) => [instance, ...list]);
        imported += 1;
      } catch (cause) {
        setError(String(cause));
      }
    }
    if (imported > 0) {
      setView("library");
      setDetail(null);
      setToast(imported === 1 ? "Инстанс импортирован" : `Импортировано: ${imported}`);
    }
  }

  async function pickImport() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Bubanya / Prism export", extensions: ["zip"] }],
      });
      if (typeof selected === "string") {
        await importArchives([selected]);
      }
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openSite() {
    const base =
      browseType === "modpack" ? "https://modrinth.com/modpacks" : "https://modrinth.com/mods";
    const query = browseSearch.trim();
    const url = query ? `${base}?q=${encodeURIComponent(query)}` : base;
    try {
      await openUrl(url);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openDataDir() {
    try {
      await openPath(dataDir);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openExportsDir() {
    try {
      await openPath(`${dataDir}/exports`);
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function clearCache() {
    if (
      !window.confirm(
        "Удалить временные файлы лаунчера? Миры, моды и инстансы останутся на месте.",
      )
    ) {
      return;
    }
    try {
      await api.clearCache();
      setToast("Кэш очищен");
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openMods(instance: Instance) {
    try {
      await openPath(await api.getModsDir(instance.id));
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function openLogs(instance: Instance) {
    try {
      await openPath(await api.getLogFile(instance.id));
    } catch (cause) {
      setError(String(cause));
    }
  }

  function openLogsTab(instance: Instance) {
    openDetail(instance);
    setDetailTab("logs");
  }

  const pageTitle =
    view === "library"
      ? "Библиотека"
      : view === "versions"
        ? "Версии"
        : view === "browse"
          ? "Обзор"
          : view === "downloads"
            ? "Загрузки"
            : view === "accounts"
              ? "Аккаунты"
              : "Настройки";

  const accent = normalizeHex(settings.accent);
  const accentStyle = {
    "--accent": accent,
    "--accent-soft": hexToRgba(accent, 0.08),
    "--accent-border": hexToRgba(accent, 0.45),
    "--accent-hover": lightenHex(accent, 0.18),
    "--on-accent": "#0a0b0d",
  } as CSSProperties;

  return (
    <div className="app" style={accentStyle}>
      <aside className="rail">
        <div className="rail-logo"><img src={logo} alt="" /></div>
        <button
          className={view === "library" ? "rail-btn active" : "rail-btn"}
          title="Библиотека"
          onClick={() => {
            setView("library");
            setDetail(null);
          }}
        >
          <IconGrid size={17} />
        </button>
        <button
          className={view === "versions" ? "rail-btn active" : "rail-btn"}
          title="Версии"
          onClick={() => {
            setView("versions");
            setDetail(null);
          }}
        >
          <IconLayers size={17} />
        </button>
        <button
          className={view === "browse" ? "rail-btn active" : "rail-btn"}
          title="Обзор Modrinth"
          onClick={() => {
            setView("browse");
            setDetail(null);
          }}
        >
          <IconCompass size={17} />
        </button>
        <button
          className={view === "downloads" ? "rail-btn active" : "rail-btn"}
          title="Загрузки"
          onClick={() => {
            setView("downloads");
            setDetail(null);
          }}
        >
          <IconDownload size={17} />
          {activeDownloads > 0 && (
            <span className="rail-badge">{activeDownloads}</span>
          )}
        </button>
        <div className="rail-sep" />
        <button
          className={view === "accounts" ? "rail-btn active" : "rail-btn"}
          title="Аккаунты"
          onClick={() => {
            setView("accounts");
            setDetail(null);
          }}
        >
          <IconUser size={17} />
        </button>
        <div className="rail-bottom">
          <button
            className={view === "settings" ? "rail-btn active" : "rail-btn"}
            title="Настройки"
            onClick={() => {
              setView("settings");
              setDetail(null);
            }}
          >
            <IconSettings size={17} />
          </button>
          <button className="rail-btn" title="Папка данных" onClick={openDataDir}>
            <IconHardDrive size={17} />
          </button>
        </div>
      </aside>

      <div className="shell">
        <header className="titlebar">
          <div className="crumbs">
            <span className="crumb-root">BUBANYA</span>
            <span className="crumb-sep">/</span>
            <span className="crumb-current">{pageTitle}</span>
            {detail && (
              <>
                <span className="crumb-sep">/</span>
                <span className="crumb-lime">{detail.name}</span>
              </>
            )}
          </div>
          <div className="titlebar-actions">
            <div className="global-search">
              <IconSearch size={15} />
              <input
                ref={searchRef}
                placeholder="Поиск по лаунчеру…"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    setLibrarySearch((event.target as HTMLInputElement).value);
                    setView("library");
                    setDetail(null);
                  }
                }}
              />
              <span className="search-key">CTRL K</span>
            </div>
            <button
              className={activeTask ? "download-pill" : "download-pill idle"}
              onClick={() => setView("downloads")}
            >
              <IconDownload size={13} />
              {activeTask ? (
                <>
                  <span className="download-pill-stage">
                    {stageLabel(activeTask.stage)}
                  </span>
                  <span className="download-pill-count">
                    {activeTask.total > 0
                      ? `${activeTask.current}/${activeTask.total}`
                      : "…"}
                  </span>
                </>
              ) : (
                <span>0 загрузок</span>
              )}
            </button>
            <button className="account-btn" onClick={() => setView("accounts")}>
              <div className="avatar small">
                {skinFor(activeAccount) ? (
                  <img src={skinFor(activeAccount)!} alt="" />
                ) : (
                  activeAccount?.name.slice(0, 1).toUpperCase() || "?"
                )}
              </div>
              <span>{activeAccount?.name ?? "Аккаунт"}</span>
            </button>
          </div>
        </header>

        {launcherUpdate?.available && launcherUpdate.latest && !launcherUpdate.dismissed && (
          <div className="update-banner">
            <span className="update-banner-icon">
              <IconArrowUpRight size={14} />
            </span>
            <span className="update-banner-text">
              Доступна Bubanya{" "}
              <b>{launcherUpdate.latest.tag.replace(/^v/, "")}</b>
              <button
                className="link-btn"
                onClick={() => void openUrl(launcherUpdate.latest!.url)}
              >
                Открыть релиз на GitHub
              </button>
            </span>
            <span className="spacer" />
            <button className="btn sm" onClick={dismissUpdateBanner}>
              <IconX size={13} /> Скрыть
            </button>
          </div>
        )}

        <main className="main">
          {view === "library" && !detail && (
            <div className="view library-view">
              <div className="page-head">
                <div>
                  <h1>Библиотека</h1>
                  <p className="subtitle">
                    {instances.length}{" "}
                    {instances.length === 1
                      ? "инстанс"
                      : instances.length < 5
                        ? "инстанса"
                        : "инстансов"}
                    {Object.keys(stats).length > 0
                      ? ` · ${totalStats.mods} модов · ${formatSize(totalStats.size)}`
                      : ""}
                    {" · "}
                    {installedCount} готовы к запуску
                  </p>
                </div>
                <div className="page-actions">
                  <button className="btn" onClick={pickImport}>
                    <IconPackage size={15} /> Импорт
                  </button>
                  <button className="btn" onClick={pickModpack}>
                    <IconLayers size={15} /> .mrpack
                  </button>
                  <button className="btn primary" onClick={() => setView("versions")}>
                    <IconPlus size={15} /> Создать
                  </button>
                </div>
              </div>

              {quickPlay && (
                <section className="featured-instance">
                  <div className="game-icon">
                    <InstanceIcon instance={quickPlay} size={24} />
                  </div>
                  <div className="featured-info">
                    <div className="featured-eyebrow">Продолжить</div>
                    <div className="featured-name">{quickPlay.name}</div>
                    <div className="game-tags">
                      <Tag>{quickPlay.versionId}</Tag>
                      {quickPlay.loader === "fabric" && (
                        <Tag tone="lime">Fabric {quickPlay.loaderVersion}</Tag>
                      )}
                      {quickPlay.loader === "forge" && (
                        <Tag>Forge {quickPlay.loaderVersion}</Tag>
                      )}
                      {quickPlay.loader !== "fabric" && quickPlay.loader !== "forge" && (
                        <Tag>Vanilla</Tag>
                      )}
                      <Tag tone="lime">готов</Tag>
                    </div>
                  </div>
                  <div className="featured-stats">
                    <div className="featured-stat">
                      Моды<b>{stats[quickPlay.id]?.mods ?? "—"}</b>
                    </div>
                    <div className="featured-stat">
                      Миры<b>{stats[quickPlay.id]?.worlds ?? "—"}</b>
                    </div>
                    <div className="featured-stat">
                      Размер
                      <b>
                        {stats[quickPlay.id]
                          ? formatSize(stats[quickPlay.id].sizeBytes)
                          : "—"}
                      </b>
                    </div>
                    <div className="featured-stat">
                      Последний запуск
                      <b>
                        {quickPlay.lastPlayed
                          ? formatRelativeDate(quickPlay.lastPlayed)
                          : "—"}
                      </b>
                    </div>
                  </div>
                  <button
                    className="btn primary"
                    disabled={busy === quickPlay.id}
                    onClick={() => play(quickPlay)}
                  >
                    <IconPlay size={16} />
                    {busy === quickPlay.id ? "Запуск…" : "Играть"}
                  </button>
                </section>
              )}

              <div className="library-toolbar">
                <div className="search">
                  <IconSearch size={15} />
                  <input
                    className="input"
                    placeholder="Поиск инстанса…"
                    value={librarySearch}
                    onChange={(event) => setLibrarySearch(event.target.value)}
                  />
                </div>
                <div className="chips">
                  {(
                    [
                      ["all", "Все"],
                      ["installed", "Установленные"],
                      ["fabric", "Fabric"],
                      ["forge", "Forge"],
                      ["vanilla", "Vanilla"],
                    ] as const
                  ).map(([id, label]) => (
                    <button
                      key={id}
                      className={libraryFilter === id ? "chip active" : "chip"}
                      onClick={() => setLibraryFilter(id)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <div className="spacer" />
                <select
                  className="input select"
                  value={librarySort}
                  onChange={(event) => setLibrarySort(event.target.value as LibrarySort)}
                >
                  <option value="recent">Сначала недавние</option>
                  <option value="name">По имени</option>
                  <option value="version">По версии</option>
                  <option value="size">По размеру</option>
                </select>
              </div>

              {filteredInstances.length === 0 ? (
                <div className="empty">
                  <div className="empty-icon">
                    <IconGrid size={34} />
                  </div>
                  <h3>Ничего не найдено</h3>
                  <p>
                    {instances.length === 0
                      ? "Создай первый инстанс: выбери версию и загрузчик."
                      : "Попробуй изменить фильтр или поисковый запрос."}
                  </p>
                  <button className="btn primary" onClick={() => setView("versions")}>
                    <IconPlus size={15} /> Создать инстанс
                  </button>
                </div>
              ) : (
                <div className="library-table">
                  <div className="library-table-head">
                    <span>Инстанс</span>
                    <span>Версия</span>
                    <span>Загрузчик</span>
                    <span>Моды</span>
                    <span>Миры</span>
                    <span>Размер</span>
                    <span>Запуск</span>
                    <span />
                  </div>
                  {filteredInstances.map((instance) => (
                    <div
                      className="library-row"
                      key={instance.id}
                      onClick={() => openDetail(instance)}
                    >
                      <div className="library-instance">
                        <div className="game-icon">
                          <InstanceIcon instance={instance} size={22} />
                        </div>
                        <div style={{ minWidth: 0 }}>
                          <div className="library-name">
                            <span
                              className={
                                instance.installed
                                  ? "lib-status ready"
                                  : "lib-status"
                              }
                            />
                            <span className="library-name-text">
                              {instance.name}
                            </span>
                            {packUpdates[instance.id]?.hasUpdate && (
                              <Tag tone="lime" title="У модпака есть новая версия">
                                обновление
                              </Tag>
                            )}
                          </div>
                          <div className="library-sub">
                            {instance.loader === "fabric"
                              ? `Fabric ${instance.loaderVersion ?? ""}`
                              : instance.loader === "forge"
                                ? `Forge ${instance.loaderVersion ?? ""}`
                                : "Vanilla"}{" "}
                            · {instance.installed ? "готов к запуску" : "не установлен"}
                          </div>
                        </div>
                      </div>
                      <span className="library-num">{instance.versionId}</span>
                      <span>
                        {instance.loader === "fabric" ? (
                          <Tag tone="lime">Fabric</Tag>
                        ) : instance.loader === "forge" ? (
                          <Tag>Forge</Tag>
                        ) : (
                          <Tag>Vanilla</Tag>
                        )}
                      </span>
                      <span className="library-num">
                        {stats[instance.id]?.mods ?? "—"}
                      </span>
                      <span className="library-num dim">
                        {stats[instance.id]?.worlds ?? "—"}
                      </span>
                      <span className="library-num">
                        {stats[instance.id]
                          ? formatSize(stats[instance.id].sizeBytes)
                          : "—"}
                      </span>
                      <span className="library-num dim">
                        {instance.lastPlayed
                          ? formatRelativeDate(instance.lastPlayed)
                          : "—"}
                      </span>
                      <div
                        className="library-actions"
                        onClick={(event) => event.stopPropagation()}
                      >
                        {instance.installed ? (
                          <button
                            className="btn primary sm"
                            disabled={busy === instance.id}
                            onClick={() => play(instance)}
                          >
                            <IconPlay size={13} />
                            {busy === instance.id ? "…" : "Играть"}
                          </button>
                        ) : (
                          <button
                            className="btn primary sm"
                            disabled={busy === instance.id}
                            onClick={() => install(instance)}
                          >
                            <IconDownload size={13} />
                            {busy === instance.id ? "…" : "Установить"}
                          </button>
                        )}
                        <IconBtn
                          title="Папка инстанса"
                          onClick={() => openFolder(instance)}
                        >
                          <IconFolder size={15} />
                        </IconBtn>
                        {instanceMenu(instance)}
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="library-bottom">
                <div className="lib-panel">
                  <div className="lib-panel-head">
                    <IconLogs size={13} /> latest.log
                    {quickPlay ? ` · ${quickPlay.name}` : ""}
                    {quickPlay && (
                      <button
                        className="btn sm"
                        onClick={() => openLogsTab(quickPlay)}
                      >
                        Все
                      </button>
                    )}
                  </div>
                  <pre className="lib-log">
                    {previewLog(libraryLog, 8) ||
                      "Лог пока пуст — запусти игру."}
                  </pre>
                </div>
                <div className="lib-panel">
                  <div className="lib-panel-head">
                    <IconDownload size={13} /> Загрузки
                    <button
                      className="btn sm"
                      onClick={() => setView("downloads")}
                    >
                      Все
                    </button>
                  </div>
                  <div className="lib-downloads">
                    {downloads.length === 0 ? (
                      <div className="lib-download-empty">
                        <IconDownload size={22} />
                        <span>Загрузок нет</span>
                      </div>
                    ) : (
                      downloads.slice(0, 2).map((item) => {
                        const percent =
                          item.total > 0
                            ? Math.min(100, (item.current / item.total) * 100)
                            : item.status === "done"
                              ? 100
                              : 5;
                        return (
                          <div className="lib-download-line" key={item.id}>
                            <span className="lib-download-name">{item.name}</span>
                            <span className="lib-download-bar">
                              <i style={{ width: `${percent}%` }} />
                            </span>
                            <span className="lib-download-pct">
                              {item.total > 0
                                ? `${item.current}/${item.total}`
                                : item.status === "done"
                                  ? "100%"
                                  : "…"}
                            </span>
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>
              </div>

            </div>
          )}

          {view === "library" && detail && (
            <div className="view detail">
              <div className="detail-head">
                <button className="btn sm" onClick={() => setDetail(null)}>
                  <IconChevronLeft size={15} /> Назад
                </button>
                <div className="detail-title">
                  <div className="game-icon big">
                    <InstanceIcon instance={detail} size={28} />
                  </div>
                  <div>
                    <h1>{detail.name}</h1>
                    <div className="game-tags">
                      <Tag>{detail.versionId}</Tag>
                      {detail.loader === "fabric" && (
                        <Tag tone="lime">Fabric {detail.loaderVersion}</Tag>
                      )}
                      {detail.loader === "forge" && (
                        <Tag>Forge {detail.loaderVersion}</Tag>
                      )}
                      <Tag tone={detail.installed ? "lime" : "red"}>
                        {detail.installed ? "установлен" : "не установлен"}
                      </Tag>
                    </div>
                  </div>
                </div>
                <div className="detail-actions">
                  <button className="btn" onClick={() => duplicate(detail)}>
                    <IconPackage size={15} /> Дублировать
                  </button>
                  <button
                    className="btn primary"
                    disabled={busy === detail.id}
                    onClick={() => play(detail)}
                  >
                    <IconPlay size={16} />
                    {busy === detail.id ? "Запуск…" : "Играть"}
                  </button>
                  <IconBtn title="Удалить инстанс" danger onClick={() => remove(detail)}>
                    <IconTrash size={16} />
                  </IconBtn>
                </div>
              </div>

              <div className="tabs">
                {DETAIL_TABS.map((tab) => (
                  <button
                    key={tab.id}
                    className={detailTab === tab.id ? "tab active" : "tab"}
                    onClick={() => setDetailTab(tab.id)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>

              <div className="detail-body">
                {detailTab === "overview" && (
                  <div className="detail-dashboard">
                    <div className="detail-main">
                      <Panel title="Статистика" icon={<IconClock size={16} />}>
                        <div className="stat-grid three">
                          <div className="stat">
                            <span className="stat-label">Последний запуск</span>
                            <span className="stat-value">
                              {detail.lastPlayed
                                ? formatRelativeDate(detail.lastPlayed)
                                : "—"}
                            </span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Создан</span>
                            <span className="stat-value">
                              {formatCompactDate(detail.createdAt)}
                            </span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Загрузчик</span>
                            <span className="stat-value">
                              {loaderLabel(detail.loader)}
                            </span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Версия</span>
                            <span className="stat-value">{detail.versionId}</span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Моды</span>
                            <span className="stat-value">
                              {stats[detail.id]?.mods ?? "—"}
                            </span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Размер</span>
                            <span className="stat-value">
                              {stats[detail.id]
                                ? formatSize(stats[detail.id].sizeBytes)
                                : "—"}
                            </span>
                          </div>
                        </div>
                      </Panel>

                      <Panel
                        title="Параметры запуска"
                        icon={<IconSliders size={16} />}
                        actions={
                          <button
                            className="btn sm"
                            onClick={() => setDetailTab("settings")}
                          >
                            Настроить
                          </button>
                        }
                      >
                        <div className="stat-grid">
                          <div className="stat">
                            <span className="stat-label">Память</span>
                            <span className="stat-value">
                              {detail.ramMb ?? settings.ramMb} МБ
                            </span>
                          </div>
                          <div className="stat">
                            <span className="stat-label">Java</span>
                            <span className="stat-value">
                              {detail.javaPath
                                ? "Своя"
                                : settings.javaPath
                                  ? "Глобальная"
                                  : "Авто"}
                            </span>
                          </div>
                        </div>
                        <div className="detail-jvm">
                          <span className="stat-label">JVM-аргументы</span>
                          <code>{detail.jvmArgs || "по умолчанию"}</code>
                        </div>
                      </Panel>

                      {packManifest && (
                        <Panel title="Модпак" icon={<IconLayers size={16} />}>
                          <div className="stat-grid">
                            <div className="stat">
                              <span className="stat-label">Версия пака</span>
                              <span className="stat-value">
                                {packManifest.name ||
                                  packManifest.versionId ||
                                  "—"}
                              </span>
                            </div>
                            <div className="stat">
                              <span className="stat-label">Источник</span>
                              <span className="stat-value">
                                {packManifest.projectId
                                  ? "Modrinth"
                                  : "Файл .mrpack"}
                              </span>
                            </div>
                            {packUpdates[detail.id]?.hasUpdate && (
                              <div className="stat">
                                <span className="stat-label">Доступна версия</span>
                                <span className="stat-value">
                                  {packUpdates[detail.id]?.latestVersionNumber ??
                                    "—"}
                                </span>
                              </div>
                            )}
                          </div>
                          <div className="quick-actions" style={{ marginTop: 8 }}>
                            <button
                              className="btn primary"
                              onClick={updatePack}
                              disabled={packBusy || !packManifest.projectId}
                              title={
                                packManifest.projectId
                                  ? "Обновить до последней версии"
                                  : "Импорт из файла нельзя обновить автоматически"
                              }
                            >
                              <IconRefresh size={15} />{" "}
                              {packBusy ? "Обновляю…" : "Обновить"}
                            </button>
                          </div>
                        </Panel>
                      )}

                      <Panel title="Быстрые действия" icon={<IconSpark size={16} />}>
                        <div className="quick-actions">
                          <button className="btn" onClick={() => openFolder(detail)}>
                            <IconFolder size={15} /> Папка
                          </button>
                          <button className="btn" onClick={() => openMods(detail)}>
                            <IconPuzzle size={15} /> Моды
                          </button>
                          <button className="btn" onClick={() => openLogs(detail)}>
                            <IconLogs size={15} /> Логи
                          </button>
                          <button className="btn" onClick={() => exportInstance(detail)}>
                            <IconPackage size={15} /> Экспорт
                          </button>
                        </div>
                      </Panel>

                      <Panel
                        title={`Моды · ${stats[detail.id]?.mods ?? installedMods.length}`}
                        icon={<IconPuzzle size={16} />}
                        actions={
                          <button
                            className="btn sm"
                            onClick={() => setDetailTab("mods")}
                          >
                            Все
                          </button>
                        }
                      >
                        {installedMods.length === 0 ? (
                          <p className="hint">Моды ещё не установлены.</p>
                        ) : (
                          installedMods.slice(0, 3).map((mod) => (
                            <div className="detail-mod-row" key={mod.filename}>
                              <span>{mod.name}</span>
                              <span className="muted">{mod.version}</span>
                            </div>
                          ))
                        )}
                      </Panel>
                    </div>

                    <div className="detail-side">
                      <Panel
                        title="Последние строки лога"
                        icon={<IconLogs size={16} />}
                        actions={
                          <button
                            className="btn sm"
                            onClick={() => setDetailTab("logs")}
                          >
                            Все
                          </button>
                        }
                      >
                        <pre className="log-preview">
                          {logText
                            ? previewLog(logText, 20)
                            : "Лог пока пуст — запусти игру."}
                        </pre>
                      </Panel>
                      <Panel
                        title="Миры"
                        icon={<IconCube size={16} />}
                        actions={
                          <button
                            className="btn sm"
                            onClick={() => setDetailTab("worlds")}
                          >
                            Все
                          </button>
                        }
                      >
                        {worlds.length === 0 ? (
                          <p className="hint">Мир пока не создан.</p>
                        ) : (
                          worlds.slice(0, 3).map((world) => (
                            <div className="detail-world-row" key={world.folder}>
                              <span>{world.name}</span>
                              <span className="muted">
                                {formatSize(world.size)}
                              </span>
                            </div>
                          ))
                        )}
                      </Panel>
                    </div>
                  </div>
                )}

                {detailTab === "mods" && (
                  <Panel
                    title={`Моды · ${installedMods.length}`}
                    icon={<IconPuzzle size={16} />}
                    actions={
                      <>
                        <button
                          className="btn sm"
                          onClick={() => {
                            setBrowseTarget(detail.id);
                            setBrowseType("mod");
                            setView("browse");
                          }}
                        >
                          <IconCompass size={14} /> Найти
                        </button>
                        <button
                          className="btn sm"
                          onClick={installModFromFile}
                          disabled={modsLoading}
                        >
                          <IconPlus size={14} /> Установить .jar
                        </button>
                        <button
                          className="btn sm"
                          onClick={() => updateMods(detail)}
                          disabled={modsLoading}
                        >
                          <IconRefresh size={14} /> {modsLoading ? "…" : "Обновить"}
                        </button>
                        <IconBtn title="Папка модов" onClick={() => openMods(detail)}>
                          <IconFolder size={14} />
                        </IconBtn>
                        <IconBtn
                          title="Переиндексировать моды"
                          onClick={() => reindexMods(detail)}
                        >
                          <IconRefresh size={14} />
                        </IconBtn>
                      </>
                    }
                  >
                    <div className="toolbar mod-toolbar">
                      <div className="search">
                        <IconSearch size={15} />
                        <input
                          className="input"
                          placeholder="Поиск мода…"
                          value={modSearch}
                          onChange={(event) => setModSearch(event.target.value)}
                        />
                      </div>
                      <div className="chips">
                        {(
                          [
                            ["all", "Все"],
                            ["enabled", "Включены"],
                            ["disabled", "Выключены"],
                          ] as const
                        ).map(([id, label]) => (
                          <button
                            key={id}
                            className={modFilter === id ? "chip active" : "chip"}
                            onClick={() => setModFilter(id)}
                          >
                            {label}
                          </button>
                        ))}
                      </div>
                      <div className="spacer" />
                      <span className="muted">
                        {enabledMods} вкл · {installedMods.length - enabledMods} выкл
                      </span>
                    </div>

                    {installedMods.length === 0 ? (
                      <div className="tab-empty">
                        <IconPuzzle size={30} />
                        <h3>Модов нет</h3>
                        <p>Найди моды на Modrinth или перетащи .jar в окно.</p>
                        <button
                          className="btn primary"
                          onClick={() => {
                            setBrowseTarget(detail.id);
                            setBrowseType("mod");
                            setView("browse");
                          }}
                        >
                          <IconCompass size={15} /> Найти моды
                        </button>
                      </div>
                    ) : filteredMods.length === 0 ? (
                      <div className="tab-empty">
                        <IconSearch size={30} />
                        <h3>Ничего не найдено</h3>
                        <p>Попробуй другой запрос или фильтр.</p>
                      </div>
                    ) : (
                      <div className="item-list">
                        {filteredMods.map((mod) => (
                          <div
                            className={mod.enabled ? "item-row" : "item-row disabled"}
                            key={mod.filename}
                          >
                            <div className="item-icon">
                              <IconPuzzle size={16} />
                            </div>
                            <div className="item-main">
                              <div className="item-name">{mod.name}</div>
                              <div className="item-sub">
                                {mod.version ? `${mod.version} · ` : ""}
                                {formatSize(mod.size)}
                                <span className="mod-file"> · {mod.filename}</span>
                              </div>
                            </div>
                            {mod.projectId && (
                              <IconBtn
                                title="Открыть на Modrinth"
                                onClick={() =>
                                  void openUrl(
                                    `https://modrinth.com/mod/${mod.projectId}`,
                                  )
                                }
                              >
                                <IconArrowUpRight size={15} />
                              </IconBtn>
                            )}
                            {mod.projectId && (
                              <IconBtn
                                title="Сменить версию"
                                onClick={() => openModVersions(mod)}
                              >
                                <IconLayers size={15} />
                              </IconBtn>
                            )}
                            <Toggle
                              on={mod.enabled}
                              onClick={() => toggleInstalledMod(mod)}
                            />
                            <IconBtn
                              title="Удалить"
                              danger
                              onClick={() => deleteInstalledMod(mod)}
                            >
                              <IconTrash size={15} />
                            </IconBtn>
                          </div>
                        ))}
                      </div>
                    )}
                  </Panel>
                )}

                {detailTab === "resourcepacks" && (
                  <Panel
                    title={`Ресурспаки · ${resourcepacks.length}`}
                    icon={<IconImage size={16} />}
                    actions={
                      <>
                        <button
                          className="btn sm"
                          onClick={() => importContent("resourcepacks")}
                        >
                          <IconPlus size={14} /> Добавить
                        </button>
                        <IconBtn
                          title="Папка ресурспаков"
                          onClick={() => openContentDir("resourcepacks")}
                        >
                          <IconFolder size={15} />
                        </IconBtn>
                      </>
                    }
                  >
                    {resourcepacks.length === 0 ? (
                      <div className="tab-empty">
                        <IconImage size={30} />
                        <h3>Ресурспаков нет</h3>
                        <p>Добавь .zip или положи файл в папку ресурспаков.</p>
                        <button
                          className="btn primary"
                          onClick={() => importContent("resourcepacks")}
                        >
                          <IconPlus size={15} /> Добавить
                        </button>
                      </div>
                    ) : (
                      <div className="item-list">
                        {resourcepacks.map((pack) => (
                          <div
                            className={pack.enabled ? "item-row" : "item-row disabled"}
                            key={pack.filename}
                          >
                            <div className="item-icon">
                              <IconImage size={16} />
                            </div>
                            <div className="item-main">
                              <div className="item-name">{pack.name}</div>
                              <div className="item-sub">
                                {formatSize(pack.size)} · {formatDate(pack.modified)}
                              </div>
                            </div>
                            <Toggle
                              on={pack.enabled}
                              onClick={() => toggleContentItem(pack, "resourcepacks")}
                            />
                            <IconBtn
                              title="Удалить"
                              danger
                              onClick={() => deleteContentItem(pack, "resourcepacks")}
                            >
                              <IconTrash size={15} />
                            </IconBtn>
                          </div>
                        ))}
                      </div>
                    )}
                  </Panel>
                )}

                {detailTab === "shaders" && (
                  <Panel
                    title={`Шейдеры · ${shaders.length}`}
                    icon={<IconSpark size={16} />}
                    actions={
                      <>
                        <button
                          className="btn sm"
                          onClick={() => importContent("shaderpacks")}
                        >
                          <IconPlus size={14} /> Добавить
                        </button>
                        <IconBtn
                          title="Папка шейдеров"
                          onClick={() => openContentDir("shaderpacks")}
                        >
                          <IconFolder size={15} />
                        </IconBtn>
                      </>
                    }
                  >
                    {shaders.length === 0 ? (
                      <div className="tab-empty">
                        <IconSpark size={30} />
                        <h3>Шейдеров нет</h3>
                        <p>Добавь .zip или положи файл в папку шейдеров.</p>
                        <button
                          className="btn primary"
                          onClick={() => importContent("shaderpacks")}
                        >
                          <IconPlus size={15} /> Добавить
                        </button>
                      </div>
                    ) : (
                      <div className="item-list">
                        {shaders.map((shader) => (
                          <div
                            className={shader.enabled ? "item-row" : "item-row disabled"}
                            key={shader.filename}
                          >
                            <div className="item-icon">
                              <IconSpark size={16} />
                            </div>
                            <div className="item-main">
                              <div className="item-name">{shader.name}</div>
                              <div className="item-sub">
                                {formatSize(shader.size)} · {formatDate(shader.modified)}
                              </div>
                            </div>
                            <Toggle
                              on={shader.enabled}
                              onClick={() => toggleContentItem(shader, "shaderpacks")}
                            />
                            <IconBtn
                              title="Удалить"
                              danger
                              onClick={() => deleteContentItem(shader, "shaderpacks")}
                            >
                              <IconTrash size={15} />
                            </IconBtn>
                          </div>
                        ))}
                      </div>
                    )}
                    <p className="hint">Для шейдеров нужен Iris или OptiFine.</p>
                  </Panel>
                )}

                {detailTab === "worlds" && (
                  <Panel
                    title={`Миры · ${worlds.length}`}
                    icon={<IconWorld size={16} />}
                    actions={
                      <>
                        <button className="btn sm" onClick={importWorldItem}>
                          <IconPlus size={14} /> Импорт
                        </button>
                        <IconBtn title="Папка миров" onClick={openWorldsDir}>
                          <IconFolder size={15} />
                        </IconBtn>
                      </>
                    }
                  >
                    {worlds.length === 0 ? (
                      <div className="tab-empty">
                        <IconWorld size={30} />
                        <h3>Миров нет</h3>
                        <p>Создай мир в игре или импортируй .zip.</p>
                        <button className="btn primary" onClick={importWorldItem}>
                          <IconPlus size={15} /> Импорт
                        </button>
                      </div>
                    ) : (
                      <div className="item-list">
                        {worlds.map((world) => (
                          <div className="item-row" key={world.folder}>
                            <div className="item-icon">
                              <IconWorld size={16} />
                            </div>
                            <div className="item-main">
                              <div className="item-name">{world.name}</div>
                              <div className="item-sub">
                                {formatSize(world.size)} ·{" "}
                                {formatDate(world.lastModified)}
                              </div>
                            </div>
                            <button
                              className="btn sm"
                              title="Запустить игру; мир выберешь в меню"
                              onClick={() => play(detail)}
                            >
                              <IconPlay size={14} /> Играть
                            </button>
                            <IconBtn
                              title="Бэкап мира"
                              onClick={() => backupWorldItem(world)}
                            >
                              <IconPackage size={15} />
                            </IconBtn>
                            <IconBtn
                              title="Удалить мир"
                              danger
                              onClick={() => deleteWorldItem(world)}
                            >
                              <IconTrash size={15} />
                            </IconBtn>
                          </div>
                        ))}
                      </div>
                    )}
                  </Panel>
                )}

                {detailTab === "logs" && (
                  <Panel
                    title="latest.log"
                    icon={<IconLogs size={16} />}
                    actions={
                      <>
                        <button
                          className="btn sm"
                          onClick={refreshLog}
                          disabled={logLoading}
                        >
                          <IconRefresh size={14} /> {logLoading ? "…" : "Обновить"}
                        </button>
                        <button className="btn sm" onClick={() => openLogs(detail)}>
                          <IconArrowUpRight size={14} /> Открыть файл
                        </button>
                      </>
                    }
                  >
                    {logText ? (
                      <pre className="log-view">{logText}</pre>
                    ) : (
                      <div className="tab-empty">
                        <IconLogs size={30} />
                        <h3>Лог пока пуст</h3>
                        <p>Запусти игру, и здесь появится latest.log.</p>
                      </div>
                    )}
                  </Panel>
                )}

                {detailTab === "settings" && (
                  <div className="detail-grid">
                    <Panel title="Инстанс" icon={<IconSliders size={16} />}>
                      <label className="field">
                        <span>Название</span>
                        <input
                          className="input"
                          value={instanceName}
                          onChange={(event) => setInstanceName(event.target.value)}
                        />
                      </label>
                      <label className="field">
                        <span>Память</span>
                        <div className="range-row">
                          <input
                            type="range"
                            min={512}
                            max={16384}
                            step={256}
                            value={instanceRam}
                            disabled={!instanceRamOverride}
                            onChange={(event) =>
                              setInstanceRam(Number(event.target.value))
                            }
                          />
                          <span className="range-value">{instanceRam} МБ</span>
                        </div>
                      </label>
                      <label className="check">
                        <input
                          type="checkbox"
                          checked={!instanceRamOverride}
                          onChange={(event) => {
                            const useGlobal = event.target.checked;
                            setInstanceRamOverride(!useGlobal);
                            if (useGlobal) {
                              setInstanceRam(settings.ramMb);
                            }
                          }}
                        />
                        <span>Использовать глобальную память ({settings.ramMb} МБ)</span>
                      </label>
                      <label className="field">
                        <span>Java</span>
                        <select
                          className="input"
                          value={instanceJava}
                          onChange={(event) => setInstanceJava(event.target.value)}
                        >
                          <option value="">Авто / как в настройках</option>
                          {java.map((item) => (
                            <option key={item.path} value={item.path}>
                              Java {item.major} — {item.version}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="field">
                        <span>JVM-аргументы</span>
                        <input
                          className="input"
                          value={instanceJvmArgs}
                          onChange={(event) => setInstanceJvmArgs(event.target.value)}
                          placeholder="-XX:+UseG1GC -XX:MaxGCPauseMillis=50"
                        />
                      </label>
                      <p className="hint">
                        Аргументы через пробел. Применяются при следующем запуске.
                      </p>
                      <div className="modal-actions" style={{ justifyContent: "flex-start" }}>
                        <button className="btn primary" onClick={saveInstanceSettings}>
                          <IconCheck size={15} /> Сохранить
                        </button>
                        <button
                          className="btn"
                          onClick={() => {
                            setInstanceName(detail.name);
                            setInstanceRam(detail.ramMb ?? settings.ramMb);
                            setInstanceRamOverride(detail.ramMb !== null);
                            setInstanceJava(detail.javaPath ?? "");
                            setInstanceJvmArgs(detail.jvmArgs ?? "");
                          }}
                        >
                          Сбросить
                        </button>
                      </div>
                    </Panel>

                    <Panel title="Опасная зона" icon={<IconShield size={16} />}>
                      <p className="hint">
                        Удаление инстанса уберёт папку с мирами и модами. Это необратимо.
                      </p>
                      <button className="btn danger" onClick={() => remove(detail)}>
                        <IconTrash size={15} /> Удалить инстанс
                      </button>
                    </Panel>
                  </div>
                )}
              </div>
            </div>
          )}

          {view === "versions" && (
            <div className="view">
              <div className="page-head">
                <div>
                  <h1>Версии</h1>
                  <p className="subtitle">Официальные версии Minecraft</p>
                </div>
                <button className="btn" onClick={loadVersions} disabled={loadingVersions}>
                  <IconRefresh size={15} />
                  {loadingVersions ? "Загрузка…" : "Обновить"}
                </button>
              </div>

              <div className="toolbar">
                <div className="search">
                  <IconSearch size={15} />
                  <input
                    className="input"
                    placeholder="Поиск версии…"
                    value={versionSearch}
                    onChange={(event) => setVersionSearch(event.target.value)}
                  />
                </div>
                <div className="chips">
                  {(
                    [
                      ["all", "Все"],
                      ["release", "Релизы"],
                      ["snapshot", "Снапшоты"],
                    ] as const
                  ).map(([id, label]) => (
                    <button
                      key={id}
                      className={versionFilter === id ? "chip active" : "chip"}
                      onClick={() => setVersionFilter(id)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <span className="muted">{filteredVersions.length}</span>
              </div>

              {versions.length === 0 ? (
                <div className="empty">
                  <div className="empty-icon">
                    <IconLayers size={34} />
                  </div>
                  <h3>Список не загружен</h3>
                  <p>Нажми «Обновить», чтобы получить версии с серверов Mojang.</p>
                  <button className="btn primary" onClick={loadVersions} disabled={loadingVersions}>
                    <IconRefresh size={15} />
                    {loadingVersions ? "Загрузка…" : "Загрузить"}
                  </button>
                </div>
              ) : (
                <div className="version-table">
                  <div className="version-table-head">
                    <span>Версия</span>
                    <span>Тип</span>
                    <span>Дата</span>
                    <span />
                  </div>
                  {filteredVersions.map((version) => (
                    <div className="version-row" key={version.id}>
                      <span className="version-id">{version.id}</span>
                      <Tag tone={version.kind === "release" ? "lime" : "dim"}>
                        {version.kind}
                      </Tag>
                      <span className="version-date">
                        {formatCompactDate(version.releaseTime)}
                      </span>
                      <button
                        className="btn sm"
                        disabled={busy === version.id}
                        onClick={() => openCreate(version)}
                      >
                        <IconPlus size={14} />
                        {busy === version.id ? "…" : "Создать"}
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {view === "browse" && (
            <div className="view">
              <div className="page-head">
                <div>
                  <h1>Обзор</h1>
                  <p className="subtitle">Моды и модпаки с Modrinth</p>
                </div>
                <button className="btn" onClick={openSite}>
                  <IconArrowUpRight size={15} /> Открыть сайт
                </button>
              </div>

              <div className="toolbar">
                <div className="search">
                  <IconSearch size={15} />
                  <input
                    className="input"
                    placeholder="Поиск мода…"
                    value={browseSearch}
                    onChange={(event) => setBrowseSearch(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        void loadBrowse(0, false);
                      }
                    }}
                  />
                </div>
                <button
                  className="btn"
                  onClick={() => loadBrowse(0, false)}
                  disabled={browseLoading || (browseType === "mod" && !targetInstance)}
                >
                  <IconSearch size={14} /> {browseLoading ? "Поиск…" : "Найти"}
                </button>
                <div className="chips">
                  <button
                    className={browseType === "mod" ? "chip active" : "chip"}
                    onClick={() => setBrowseType("mod")}
                  >
                    Моды
                  </button>
                  <button
                    className={browseType === "modpack" ? "chip active" : "chip"}
                    onClick={() => setBrowseType("modpack")}
                  >
                    Модпаки
                  </button>
                </div>
                <div className="spacer" />
                <select
                  className="input select"
                  value={browseSort}
                  onChange={(event) => setBrowseSort(event.target.value)}
                >
                  <option value="relevance">По релевантности</option>
                  <option value="downloads">По загрузкам</option>
                  <option value="newest">Новые</option>
                  <option value="updated">Обновлённые</option>
                </select>
                {browseType === "mod" && instances.length > 0 && (
                  <select
                    className="input select"
                    value={browseTarget}
                    onChange={(event) => setBrowseTarget(event.target.value)}
                  >
                    {instances.map((instance) => (
                      <option key={instance.id} value={instance.id}>
                        {instance.name} · {instance.versionId}
                      </option>
                    ))}
                  </select>
                )}
                {browseType === "mod" && targetInstance && (
                  <span className="muted">
                    {browseInstalled.length} установлено
                  </span>
                )}
                <span className="muted">
                  {browseTotal.toLocaleString("ru-RU")} найдено
                </span>
              </div>

              {browseType === "mod" && !targetInstance ? (
                <div className="empty">
                  <div className="empty-icon">
                    <IconPackage size={34} />
                  </div>
                  <h3>Нужен инстанс</h3>
                  <p>Создай инстанс, чтобы устанавливать в него моды с Modrinth.</p>
                  <button className="btn primary" onClick={() => setView("versions")}>
                    <IconPlus size={15} /> Создать инстанс
                  </button>
                </div>
              ) : browseResults.length === 0 ? (
                <div className="empty">
                  <div className="empty-icon">
                    <IconCompass size={34} />
                  </div>
                  <h3>{browseLoading ? "Ищем…" : "Ничего не найдено"}</h3>
                  <p>
                    {browseLoading
                      ? "Запрашиваем Modrinth…"
                      : browseType === "modpack"
                        ? "Модпаков не найдено. Попробуй другой запрос."
                        : `Для ${targetInstance?.versionId ?? "—"} / ${
                            loaderLabel(targetInstance?.loader ?? "vanilla")
                          } ничего нет. Попробуй другой запрос.`}
                  </p>
                </div>
              ) : (
                <>
                  <div className="browse-grid">
                    {browseResults.map((item) => (
                      <article className="browse-card" key={item.id}>
                        <div className="browse-cover">
                          {item.iconUrl ? (
                            <img src={item.iconUrl} alt="" loading="lazy" />
                          ) : (
                            <IconPackage size={34} />
                          )}
                        </div>
                        <div className="browse-body">
                          <div className="browse-title">
                            <h3>{item.title}</h3>
                            {item.categories
                              .filter(
                                (category) =>
                                  ![
                                    "fabric",
                                    "forge",
                                    "neoforge",
                                    "quilt",
                                    "liteloader",
                                    "modloader",
                                    "rift",
                                  ].includes(category),
                              )
                              .slice(0, 2)
                              .map((category) => (
                                <Tag key={category}>{category}</Tag>
                              ))}
                          </div>
                          <p>{item.description}</p>
                          <div className="browse-foot">
                            <span className="muted">
                              <IconDownload size={13} /> {formatDownloads(item.downloads)}
                              {item.author ? ` · ${item.author}` : ""}
                            </span>
                            {(() => {
                              if (browseType === "modpack") {
                                const entry = Object.entries(packManifests).find(
                                  ([, manifest]) =>
                                    manifest.projectId === item.id,
                                );
                                if (entry) {
                                  return (
                                    <button
                                      className="btn sm installed"
                                      title="Открыть установленный модпак"
                                      onClick={() => openPackInstance(entry[0])}
                                    >
                                      <IconCheck size={14} /> Установлено
                                    </button>
                                  );
                                }
                              }
                              const installed = browseType === "mod" && isInstalled(item);
                              return (
                                <button
                                  className={installed ? "btn sm installed" : "btn sm"}
                                  title={installed ? "Сменить версию" : "Выбрать версию"}
                                  onClick={() => openVersions(item)}
                                >
                                  {installed ? (
                                    <>
                                      <IconCheck size={14} /> Установлено
                                    </>
                                  ) : (
                                    <>
                                      <IconDownload size={14} /> Установить
                                    </>
                                  )}
                                </button>
                              );
                            })()}
                          </div>
                        </div>
                      </article>
                    ))}
                  </div>
                  {browseResults.length < browseTotal && (
                    <div className="load-more">
                      <button
                        className="btn"
                        onClick={() => loadBrowse(browseResults.length, true)}
                        disabled={browseLoading}
                      >
                        <IconRefresh size={14} />
                        {browseLoading ? "Загрузка…" : "Показать ещё"}
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          )}

          {view === "downloads" && (
            <div className="view downloads-view">
              <div className="page-head">
                <div>
                  <h1>Загрузки</h1>
                  <p className="subtitle">
                    {activeDownloads} активных · {downloads.length} всего
                  </p>
                </div>
                <button
                  className="btn"
                  disabled={downloads.length === 0}
                  onClick={() =>
                    setDownloads((list) =>
                      list.filter(
                        (item) =>
                          item.status === "active" || item.status === "paused",
                      ),
                    )
                  }
                >
                  <IconTrash size={15} /> Очистить завершённые
                </button>
              </div>

              {downloads.length === 0 ? (
                <div className="empty">
                  <div className="empty-icon">
                    <IconDownload size={34} />
                  </div>
                  <h3>Загрузок нет</h3>
                  <p>Установка инстансов и модов будет видна здесь и не заблокирует лаунчер.</p>
                  <button className="btn primary" onClick={() => setView("versions")}>
                    <IconPlus size={15} /> Создать инстанс
                  </button>
                </div>
              ) : (
                <div className="download-list">
                  {downloads.map((item) => {
                    const percent =
                      item.total > 0
                        ? Math.min(100, (item.current / item.total) * 100)
                        : item.status === "done"
                          ? 100
                          : 5;
                    return (
                      <div className="download-row" key={item.id}>
                        <div
                          className={
                            item.status === "error" ? "download-icon error" : "download-icon"
                          }
                        >
                          {item.status === "done" ? (
                            <IconCheck size={16} />
                          ) : item.status === "error" ? (
                            <IconX size={16} />
                          ) : item.status === "paused" ? (
                            <IconPause size={16} />
                          ) : (
                            <IconDownload size={16} />
                          )}
                        </div>
                        <div className="download-main">
                          <div className="download-name">{item.name}</div>
                          <div className="download-meta">
                            {item.version} · {stageLabel(item.stage)}
                            {item.total > 0 ? ` · ${item.current} / ${item.total}` : ""}
                            {item.status === "done"
                              ? " · завершено"
                              : item.status === "error"
                                ? " · ошибка"
                                : item.status === "paused"
                                  ? " · пауза"
                                  : ""}
                          </div>
                          <div className="progress-track">
                            <div
                              className={
                                item.status === "error"
                                  ? "progress-fill error"
                                  : "progress-fill"
                              }
                              style={{ width: `${percent}%` }}
                            />
                          </div>
                        </div>
                        <div className="download-actions">
                          {item.status === "active" ? (
                            <>
                              <IconBtn
                                title="Пауза"
                                onClick={() => pauseDownload(item.id)}
                              >
                                <IconPause size={15} />
                              </IconBtn>
                              <IconBtn
                                title="Отменить загрузку"
                                danger
                                onClick={() => cancelDownload(item.id)}
                              >
                                <IconX size={15} />
                              </IconBtn>
                            </>
                          ) : item.status === "paused" ? (
                            <>
                              <IconBtn
                                title="Продолжить"
                                onClick={() => resumeDownload(item.id)}
                              >
                                <IconPlay size={15} />
                              </IconBtn>
                              <IconBtn
                                title="Отменить загрузку"
                                danger
                                onClick={() => cancelDownload(item.id)}
                              >
                                <IconX size={15} />
                              </IconBtn>
                            </>
                          ) : (
                            <IconBtn
                              title="Убрать"
                              danger
                              onClick={() =>
                                setDownloads((list) =>
                                  list.filter((entry) => entry.id !== item.id),
                                )
                              }
                            >
                              <IconX size={15} />
                            </IconBtn>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {view === "accounts" && (
            <div className="view">
              <div className="page-head">
                <div>
                  <h1>Аккаунты</h1>
                  <p className="subtitle">Оффлайн-профили и Microsoft</p>
                </div>
                <div className="page-actions">
                  <button className="btn" onClick={openMicrosoft}>
                    <IconMicrosoft size={15} /> Microsoft
                  </button>
                  <button className="btn primary" onClick={openAddAccount}>
                    <IconPlus size={15} /> Добавить оффлайн
                  </button>
                </div>
              </div>

              <div className="accounts-layout">
                <div className="accounts-main">
                  {accounts.accounts.length === 0 ? (
                    <div className="empty">
                      <div className="empty-icon">
                        <IconUser size={34} />
                      </div>
                      <h3>Нет аккаунтов</h3>
                      <p>Добавь оффлайн-ник, чтобы запускать игру.</p>
                      <button className="btn primary" onClick={openAddAccount}>
                        <IconPlus size={15} /> Добавить аккаунт
                      </button>
                    </div>
                  ) : (
                    <div className="account-list">
                      {accounts.accounts.map((account) => {
                        const active = account.id === accounts.activeId;
                        const microsoft = account.kind === "microsoft";
                        return (
                          <div
                            className={active ? "account-card active" : "account-card"}
                            key={account.id}
                          >
                            <div
                              className={
                                microsoft
                                  ? "account-avatar microsoft"
                                  : "account-avatar"
                              }
                            >
                              {skinFor(account) ? (
                                <img src={skinFor(account)!} alt="" />
                              ) : microsoft ? (
                                <IconMicrosoft size={18} />
                              ) : (
                                account.name.slice(0, 1).toUpperCase() || "?"
                              )}
                            </div>
                            <div className="account-main">
                              <div className="account-name">{account.name}</div>
                              <div className="account-sub">
                                {microsoft ? "Microsoft" : "Оффлайн"} · UUID{" "}
                                {account.uuid.slice(0, 8)}…
                              </div>
                            </div>
                            {active ? (
                              <Tag tone="lime">активен</Tag>
                            ) : (
                              <button
                                className="btn sm"
                                onClick={() => selectAccount(account)}
                              >
                                <IconCheck size={14} /> Выбрать
                              </button>
                            )}
                            <div className="account-actions">
                              {!microsoft && (
                                <IconBtn
                                  title="Переименовать"
                                  onClick={() => openRenameAccount(account)}
                                >
                                  <IconSliders size={16} />
                                </IconBtn>
                              )}
                              <IconBtn
                                title="Удалить"
                                danger
                                onClick={() => removeAccount(account)}
                              >
                                <IconTrash size={16} />
                              </IconBtn>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}

                  <div className="account-card microsoft-connect">
                    <div className="account-avatar microsoft">
                      <IconMicrosoft size={20} />
                    </div>
                    <div className="account-main">
                      <div className="account-name">Microsoft</div>
                      <div className="account-sub">Лицензия, скины, Realms</div>
                    </div>
                    <button className="btn sm" onClick={openMicrosoft}>
                      <IconArrowUpRight size={14} /> Подключить
                    </button>
                  </div>
                </div>

                <div className="accounts-side">
                  <Panel title="Активный аккаунт" icon={<IconUser size={16} />}>
                    {activeAccount ? (
                      <div className="active-account">
                        <div className="active-account-avatar">
                          {skinFor(activeAccount) ? (
                            <img src={skinFor(activeAccount)!} alt="" />
                          ) : (
                            activeAccount.name.slice(0, 1).toUpperCase() || "?"
                          )}
                        </div>
                        <div>
                          <div className="account-name">{activeAccount.name}</div>
                          <div className="account-sub">
                            {activeAccount.kind === "microsoft"
                              ? "Microsoft"
                              : "Оффлайн"}{" "}
                            · UUID {activeAccount.uuid.slice(0, 8)}…
                          </div>
                        </div>
                      </div>
                    ) : (
                      <p className="hint">Аккаунт не выбран.</p>
                    )}
                  </Panel>

                  <Panel title="Как это работает" icon={<IconUser size={16} />}>
                    <p className="hint">
                      Оффлайн-аккаунт использует ник и UUID, вычисленный из него.
                      Подходит для одиночной игры и серверов с offline-mode. Для
                      лицензионных серверов подключи Microsoft: вход по коду, токены
                      хранятся в системном keyring, а не в файлах.
                    </p>
                  </Panel>
                </div>
              </div>
            </div>
          )}

          {view === "settings" && (
            <div className="view">
              <div className="page-head">
                <div>
                  <h1>Настройки</h1>
                  <p className="subtitle">Игра, лаунчер и данные</p>
                </div>
                <button className="btn primary" onClick={saveSettings}>
                  <IconCheck size={15} /> Сохранить
                </button>
              </div>

              <div className="settings-grid">
                <Panel title="Игра" icon={<IconChip size={16} />}>
                  <label className="field">
                    <span>Память</span>
                    <div className="range-row">
                      <input
                        type="range"
                        min={1024}
                        max={8192}
                        step={512}
                        value={settings.ramMb}
                        onChange={(event) =>
                          setSettings({ ...settings, ramMb: Number(event.target.value) })
                        }
                      />
                      <span className="range-value">{settings.ramMb} МБ</span>
                    </div>
                  </label>
                  <label className="field">
                    <span>Java</span>
                    <select
                      className="input"
                      value={settings.javaPath ?? ""}
                      onChange={(event) =>
                        setSettings({ ...settings, javaPath: event.target.value || null })
                      }
                    >
                      <option value="">Авто (первая найденная)</option>
                      {java.map((item) => (
                        <option key={item.path} value={item.path}>
                          Java {item.major} — {item.version}
                        </option>
                      ))}
                    </select>
                  </label>
                  <div className="java-managed">
                    <span className="muted">Скачать JRE:</span>
                    {[8, 17, 21, 25].map((major) => {
                      const installed = hasManagedJava(major);
                      return (
                        <button
                          key={major}
                          className={installed ? "btn sm installed" : "btn sm"}
                          disabled={javaInstalling !== null}
                          title={
                            installed
                              ? `Удалить скачанную Java ${major}`
                              : `Скачать Adoptium JRE ${major}`
                          }
                          onClick={() =>
                            installed ? removeJava(major) : installJava(major)
                          }
                        >
                          {javaInstalling === major
                            ? "…"
                            : installed
                              ? `Java ${major} ✓`
                              : `Java ${major}`}
                        </button>
                      );
                    })}
                  </div>
                  <p className="hint">
                    Найдено: {java.length}. Для 26.3 нужна Java 25. Скачанные JRE
                    лежат в папке лаунчера.
                  </p>
                </Panel>

                <Panel title="Сеть" icon={<IconDownload size={16} />}>
                  <label className="field">
                    <span>Параллельных загрузок</span>
                    <div className="range-row">
                      <input
                        type="range"
                        min={1}
                        max={24}
                        step={1}
                        value={settings.concurrency}
                        onChange={(event) =>
                          setSettings({ ...settings, concurrency: Number(event.target.value) })
                        }
                      />
                      <span className="range-value">{settings.concurrency}</span>
                    </div>
                  </label>
                  <p className="hint">
                    Больше потоков — быстрее установка, но выше нагрузка на сеть.
                  </p>
                  <div className="settings-info">
                    <div><span>SHA1 библиотек</span><b>всегда</b></div>
                    <div><span>Ассеты</span><b>при установке</b></div>
                    <div><span>Повторные попытки</span><b>3 на файл</b></div>
                  </div>
                </Panel>

                <Panel title="Лаунчер" icon={<IconSettings size={16} />}>
                  <div className="field">
                    <span>Акцент</span>
                    <div className="accent-row">
                      {ACCENTS.map((item) => (
                        <button
                          key={item.color}
                          className={
                            accent.toLowerCase() === item.color.toLowerCase()
                              ? "swatch active"
                              : "swatch"
                          }
                          style={{ background: item.color }}
                          title={item.name}
                          onClick={() => setSettings({ ...settings, accent: item.color })}
                        />
                      ))}
                      <label className="swatch custom" title="Свой цвет">
                        <input
                          type="color"
                          value={accent}
                          onChange={(event) =>
                            setSettings({ ...settings, accent: event.target.value })
                          }
                        />
                      </label>
                    </div>
                  </div>
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={settings.closeOnLaunch}
                      onChange={(event) =>
                        setSettings({ ...settings, closeOnLaunch: event.target.checked })
                      }
                    />
                    <span>Закрывать лаунчер после запуска</span>
                  </label>
                </Panel>

                <Panel title="Данные" icon={<IconHardDrive size={16} />}>
                  <div className="data-dir">
                    <span className="muted">Папка:</span>
                    <code>{dataDir}</code>
                  </div>
                  <div className="quick-actions">
                    <button className="btn" onClick={openDataDir}>
                      <IconFolder size={15} /> Открыть
                    </button>
                    <button className="btn" onClick={openExportsDir}>
                      <IconPackage size={15} /> Экспорты
                    </button>
                    <button className="btn" onClick={clearCache}>
                      <IconTrash size={15} /> Очистить кэш
                    </button>
                  </div>
                </Panel>

                <Panel wide title="О лаунчере" icon={<IconShield size={16} />}>
                  <div className="about-brand">
                    <img src={logo} alt="" />
                    <div>
                      <strong>Bubanya</strong>
                      <span className="muted">Minecraft launcher</span>
                    </div>
                  </div>
                  <div className="about">
                    <div>
                      <span className="muted">Версия</span>
                      <strong>{appVersion}</strong>
                    </div>
                    <div>
                      <span className="muted">Сборка</span>
                      <strong>Tauri 2 · React 19</strong>
                    </div>
                    <div>
                      <span className="muted">Платформа</span>
                      <strong>
                        {navigator.userAgent.includes("Windows")
                          ? "Windows · x86_64"
                          : navigator.userAgent.includes("Mac")
                            ? "macOS"
                            : "Linux · x86_64"}
                      </strong>
                    </div>
                  </div>
                </Panel>
              </div>
            </div>
          )}

          {error && (
            <div className="error-bar" onClick={() => setError(null)}>
              {error}
            </div>
          )}
        </main>

        <footer className="statusbar">
          <span className="status-item">
            <IconChip size={13} /> Java {java[0]?.major ?? "—"}
          </span>
          <span className="status-item">
            <IconSliders size={13} /> {settings.ramMb} МБ
          </span>
          <span className="status-item">
            <IconDownload size={13} /> {activeDownloads} загрузок
          </span>
          <span className="status-item path" title={dataDir}>
            <IconHardDrive size={13} /> {dataDir}
          </span>
          <span className="status-item right">v{appVersion}</span>
        </footer>
      </div>

      {versionTarget && (
        <div className="modal-overlay" onClick={() => setVersionTarget(null)}>
          <div className="modal version-modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-head">
              <div>
                <h3>{versionTarget.title}</h3>
                <p className="muted">
                  {versionTarget.type === "modpack" ? "Версии модпака" : "Версии мода"}
                </p>
              </div>
              <IconBtn title="Закрыть" onClick={() => setVersionTarget(null)}>
                <IconX size={15} />
              </IconBtn>
            </div>
            {versionsLoading ? (
              <div className="version-loading">
                <div className="spinner" /> Загрузка версий…
              </div>
            ) : versionList.length === 0 ? (
              <p className="hint">
                Нет совместимых версий
                {versionTarget.type === "mod" ? " для этого инстанса" : ""}.
              </p>
            ) : (
              <div className="version-picker">
                {versionList.map((version) => (
                  <div className="version-choice" key={version.id}>
                    <div className="version-choice-main">
                      <div className="version-choice-title">{version.versionNumber}</div>
                      <div className="version-choice-sub">
                        {version.name}
                        {version.gameVersions.length > 0
                          ? ` · ${version.gameVersions.join(", ")}`
                          : ""}
                        {version.loaders.length > 0 ? ` · ${version.loaders.join(", ")}` : ""}
                        {version.size > 0 ? ` · ${formatSize(version.size)}` : ""}
                      </div>
                      {version.date && (
                        <div className="version-choice-date">{formatDate(version.date)}</div>
                      )}
                    </div>
                    <button
                      className="btn sm primary"
                      disabled={installingVersion !== null}
                      onClick={() => installChosenVersion(version)}
                    >
                      {installingVersion === version.id ? "…" : "Установить"}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {creatingVersion && (
        <div className="modal-overlay" onClick={() => setCreatingVersion(null)}>
          <div className="modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-head">
              <h3>Создать инстанс</h3>
              <IconBtn title="Закрыть" onClick={() => setCreatingVersion(null)}>
                <IconX size={15} />
              </IconBtn>
            </div>
            <p className="muted">
              Версия <strong>{creatingVersion.id}</strong>
            </p>
            <div className="loader-choice">
              <label className={loaderKind === "vanilla" ? "loader-option active" : "loader-option"}>
                <input
                  type="radio"
                  name="loader"
                  checked={loaderKind === "vanilla"}
                  onChange={() => setLoaderKind("vanilla")}
                />
                <span>Vanilla</span>
              </label>
              <label className={loaderKind === "fabric" ? "loader-option active" : "loader-option"}>
                <input
                  type="radio"
                  name="loader"
                  checked={loaderKind === "fabric"}
                  onChange={() => setLoaderKind("fabric")}
                />
                <span>Fabric</span>
              </label>
              <label className={loaderKind === "forge" ? "loader-option active" : "loader-option"}>
                <input
                  type="radio"
                  name="loader"
                  checked={loaderKind === "forge"}
                  onChange={() => setLoaderKind("forge")}
                />
                <span>Forge</span>
              </label>
            </div>
            {loaderKind === "fabric" && (
              <label className="field">
                <span>Загрузчик Fabric</span>
                <select
                  className="input"
                  value={fabricLoader}
                  onChange={(event) => setFabricLoader(event.target.value)}
                  disabled={loadingLoaders || fabricLoaders.length === 0}
                >
                  {loadingLoaders && <option value="">Загрузка…</option>}
                  {!loadingLoaders && fabricLoaders.length === 0 && (
                    <option value="">Нет загрузчиков</option>
                  )}
                  {fabricLoaders.map((item) => (
                    <option key={item.version} value={item.version}>
                      {item.version}
                      {item.stable ? " (stable)" : ""}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {loaderKind === "forge" && (
              <label className="field">
                <span>Загрузчик Forge</span>
                <select
                  className="input"
                  value={forgeVersion}
                  onChange={(event) => setForgeVersion(event.target.value)}
                  disabled={loadingForge || forgeVersions.length === 0}
                >
                  {loadingForge && <option value="">Загрузка…</option>}
                  {!loadingForge && forgeVersions.length === 0 && (
                    <option value="">Нет версий</option>
                  )}
                  {forgeVersions.map((item) => (
                    <option key={item.version} value={item.version}>
                      {item.version}
                      {item.recommended ? " (recommended)" : ""}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <div className="modal-actions">
              <button className="btn" onClick={() => setCreatingVersion(null)}>
                Отмена
              </button>
              <button
                className="btn primary"
                onClick={confirmCreate}
                disabled={
                  (loaderKind === "fabric" && !fabricLoader) ||
                  (loaderKind === "forge" && !forgeVersion)
                }
              >
                <IconDownload size={15} /> Создать и установить
              </button>
            </div>
          </div>
        </div>
      )}

      {microsoftOpen && (
        <div className="modal-overlay" onClick={closeMicrosoft}>
          <div className="modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-head">
              <h3>Вход Microsoft</h3>
              <IconBtn title="Закрыть" onClick={closeMicrosoft}>
                <IconX size={15} />
              </IconBtn>
            </div>
            {microsoftError ? (
              <>
                <p className="hint" style={{ color: "var(--red)" }}>
                  {microsoftError}
                </p>
                <div className="modal-actions">
                  <button className="btn" onClick={closeMicrosoft}>
                    Закрыть
                  </button>
                  <button className="btn primary" onClick={() => void openMicrosoft()}>
                    Повторить
                  </button>
                </div>
              </>
            ) : !microsoftCode ? (
              <div className="version-loading">
                <div className="spinner" /> Получаем код…
              </div>
            ) : (
              <>
                <p className="hint">
                  Открой <strong>microsoft.com/link</strong> и введи код:
                </p>
                <div className="device-code">{microsoftCode.userCode}</div>
                <p className="hint">
                  {microsoftCode.message || "Код действует несколько минут."}
                </p>
                <div className="modal-actions">
                  <button className="btn" onClick={closeMicrosoft}>
                    Отмена
                  </button>
                  <button
                    className="btn primary"
                    onClick={() => void openUrl(microsoftCode.verificationUri)}
                  >
                    <IconArrowUpRight size={15} /> Открыть страницу
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {accountModal && (
        <div className="modal-overlay" onClick={() => setAccountModal(null)}>
          <div className="modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-head">
              <h3>
                {accountModal === "add" ? "Новый оффлайн-аккаунт" : "Переименовать аккаунт"}
              </h3>
              <IconBtn title="Закрыть" onClick={() => setAccountModal(null)}>
                <IconX size={15} />
              </IconBtn>
            </div>
            <label className="field">
              <span>Ник</span>
              <input
                className="input"
                autoFocus
                value={accountName}
                onChange={(event) => setAccountName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    void saveAccount();
                  }
                  if (event.key === "Escape") {
                    setAccountModal(null);
                  }
                }}
                placeholder="3–16 символов: латиница, цифры, _"
              />
            </label>
            <p className="hint">
              UUID пересчитывается из ника. Если мир уже создан, лучше не менять имя
              игрока — иначе изменится UUID.
            </p>
            <div className="modal-actions">
              <button className="btn" onClick={() => setAccountModal(null)}>
                Отмена
              </button>
              <button
                className="btn primary"
                disabled={accountBusy}
                onClick={() => void saveAccount()}
              >
                {accountBusy
                  ? "…"
                  : accountModal === "add"
                    ? "Добавить"
                    : "Сохранить"}
              </button>
            </div>
          </div>
        </div>
      )}

      {renameTarget && (
        <div className="modal-overlay" onClick={() => setRenameTarget(null)}>
          <div className="modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-head">
              <h3>Переименовать инстанс</h3>
              <IconBtn title="Закрыть" onClick={() => setRenameTarget(null)}>
                <IconX size={15} />
              </IconBtn>
            </div>
            <label className="field">
              <span>Имя</span>
              <input
                className="input"
                autoFocus
                value={renameValue}
                onChange={(event) => setRenameValue(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    void confirmRename();
                  }
                  if (event.key === "Escape") {
                    setRenameTarget(null);
                  }
                }}
              />
            </label>
            <div className="modal-actions">
              <button className="btn" onClick={() => setRenameTarget(null)}>
                Отмена
              </button>
              <button className="btn primary" onClick={() => void confirmRename()}>
                Сохранить
              </button>
            </div>
          </div>
        </div>
      )}

      {dragActive && (
        <div className="drop-overlay">
          <div className="drop-box">
            {dragKind === "mod" ? (
              <IconPuzzle size={36} />
            ) : dragKind === "modpack" ? (
              <IconLayers size={36} />
            ) : dragKind === "content" ? (
              <IconImage size={36} />
            ) : dragKind === "world" ? (
              <IconCube size={36} />
            ) : (
              <IconPackage size={36} />
            )}
            <h3>
              {dragKind === "mod"
                ? "Отпусти .jar"
                : dragKind === "modpack"
                  ? "Отпусти .mrpack"
                  : "Отпусти .zip"}
            </h3>
            <p>
              {dragKind === "mod"
                ? "Установка мода в текущий инстанс"
                : dragKind === "modpack"
                  ? "Импорт модпака Modrinth в новый инстанс"
                  : dragKind === "content"
                    ? "Ресурспак или шейдер в текущий инстанс"
                    : dragKind === "world"
                      ? "Импорт мира в текущий инстанс"
                      : "Экспорт Bubanya или Prism/MultiMC"}
            </p>
          </div>
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}

      {booting && (
        <div className="boot">
          <div className="boot-logo"><img src={logo} alt="" /></div>
          <div className="boot-name">BUBANYA</div>
          <div className="boot-bar">
            <span />
          </div>
          <div className="boot-note">Инициализация…</div>
        </div>
      )}
    </div>
  );
}

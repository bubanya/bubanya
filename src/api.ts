import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Accounts,
  ContentFile,
  DeviceCodeInfo,
  FabricLoader,
  ForgeVersion,
  InstalledMod,
  Instance,
  InstanceStats,
  JavaCheck,
  JavaInfo,
  LaunchInfo,
  ModrinthSearchResult,
  MicrosoftPollResult,
  ModrinthVersion,
  PackManifest,
  Progress,
  Settings,
  VersionSummary,
  WorldInfo,
} from "./types";

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  listInstances: () => invoke<Instance[]>("list_instances"),
  listAccounts: () => invoke<Accounts>("list_accounts"),
  getAccountSkin: (uuid: string) =>
    invoke<string | null>("get_account_skin", { uuid }),
  addOfflineAccount: (name: string) =>
    invoke<Accounts>("add_offline_account", { name }),
  renameAccount: (id: string, name: string) =>
    invoke<Accounts>("rename_account", { id, name }),
  deleteAccount: (id: string) => invoke<Accounts>("delete_account", { id }),
  setActiveAccount: (id: string) => invoke<Accounts>("set_active_account", { id }),
  updateInstanceSettings: (
    id: string,
    name: string,
    ramMb: number | null,
    javaPath: string | null,
    jvmArgs: string | null,
  ) =>
    invoke<Instance>("update_instance_settings", {
      id,
      name,
      ramMb,
      javaPath,
      jvmArgs,
    }),
  microsoftBeginLogin: () => invoke<DeviceCodeInfo>("microsoft_begin_login"),
  microsoftPollLogin: (deviceCode: string) =>
    invoke<MicrosoftPollResult>("microsoft_poll_login", { deviceCode }),
  createInstance: (
    name: string,
    versionId: string,
    loader: string,
    loaderVersion: string | null,
  ) => invoke<Instance>("create_instance", { name, versionId, loader, loaderVersion }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  fetchVersions: () => invoke<VersionSummary[]>("fetch_versions"),
  fetchFabricLoaders: (gameVersion: string) =>
    invoke<FabricLoader[]>("fetch_fabric_loaders", { gameVersion }),
  fetchForgeVersions: (gameVersion: string) =>
    invoke<ForgeVersion[]>("fetch_forge_versions", { gameVersion }),
  installInstance: (id: string) => invoke<Instance>("install_instance", { id }),
  launchInstance: (id: string) => invoke<LaunchInfo>("launch_instance", { id }),
  detectJava: () => invoke<JavaInfo[]>("detect_java"),
  checkInstanceJava: (id: string) =>
    invoke<JavaCheck>("check_instance_java", { id }),
  installJava: (major: number) => invoke<JavaInfo>("install_java", { major }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  pauseDownload: (id: string) => invoke<void>("pause_download", { id }),
  resumeDownload: (id: string) => invoke<void>("resume_download", { id }),
  removeJava: (major: number) => invoke<void>("remove_java", { major }),
  getDataDir: () => invoke<string>("get_data_dir"),
  clearCache: () => invoke<void>("clear_cache"),
  getInstanceDir: (id: string) => invoke<string>("get_instance_dir", { id }),
  getModsDir: (id: string) => invoke<string>("get_mods_dir", { id }),
  getLogFile: (id: string) => invoke<string>("get_log_file", { id }),
  readLog: (id: string, lines = 500) =>
    invoke<string>("read_log", { id, lines }),
  instanceStats: (id: string) => invoke<InstanceStats>("instance_stats", { instanceId: id }),
  renameInstance: (id: string, name: string) =>
    invoke<Instance>("rename_instance", { id, name }),
  duplicateInstance: (id: string) => invoke<Instance>("duplicate_instance", { id }),
  exportInstance: (id: string) => invoke<string>("export_instance", { id }),
  exportInstanceGame: (id: string) => invoke<string>("export_instance_game", { id }),
  importInstance: (path: string) => invoke<Instance>("import_instance", { path }),
  importModpackFile: (path: string, taskId: string) =>
    invoke<Instance>("import_modpack_file", { path, taskId }),
  getPackManifest: (instanceId: string) =>
    invoke<PackManifest | null>("get_pack_manifest", { instanceId }),
  listPackManifests: () =>
    invoke<Record<string, PackManifest>>("list_pack_manifests"),
  updateModpack: (id: string, versionId: string | null) =>
    invoke<Instance>("update_modpack", { id, versionId }),
  onProgress: (callback: (progress: Progress) => void) =>
    listen<Progress>("install-progress", (event) => callback(event.payload)),
};

export const modrinth = {
  search: (
    query: string,
    gameVersion: string,
    loader: string,
    projectType: string,
    sort: string,
    offset: number,
    limit: number,
  ) =>
    invoke<ModrinthSearchResult>("modrinth_search", {
      query,
      gameVersion,
      loader,
      projectType,
      sort,
      offset,
      limit,
    }),
  versions: (projectId: string, gameVersion: string, loader: string) =>
    invoke<ModrinthVersion[]>("modrinth_versions", { projectId, gameVersion, loader }),
  installVersion: (instanceId: string, versionId: string) =>
    invoke<InstalledMod>("modrinth_install_version", { instanceId, versionId }),
  installModpackVersion: (projectId: string, versionId: string) =>
    invoke<Instance>("modrinth_install_modpack_version", { projectId, versionId }),
  install: (instanceId: string, projectId: string, gameVersion: string, loader: string) =>
    invoke<InstalledMod>("modrinth_install", {
      instanceId,
      projectId,
      gameVersion,
      loader,
    }),
  installModpack: (projectId: string) =>
    invoke<Instance>("modrinth_install_modpack", { projectId }),
  updateAll: (instanceId: string, gameVersion: string, loader: string) =>
    invoke<InstalledMod[]>("modrinth_update_all", { instanceId, gameVersion, loader }),
  reindex: (instanceId: string) =>
    invoke<InstalledMod[]>("modrinth_reindex", { instanceId }),
  listMods: (instanceId: string) =>
    invoke<InstalledMod[]>("list_mods", { instanceId }),
  installModFile: (instanceId: string, path: string) =>
    invoke<InstalledMod>("install_mod_file", { instanceId, path }),
  toggleMod: (instanceId: string, filename: string) =>
    invoke<boolean>("toggle_mod", { instanceId, filename }),
  deleteMod: (instanceId: string, filename: string) =>
    invoke<void>("delete_mod", { instanceId, filename }),
};

export const content = {
  list: (instanceId: string, kind: string) =>
    invoke<ContentFile[]>("list_content", { instanceId, kind }),
  toggle: (instanceId: string, kind: string, filename: string) =>
    invoke<boolean>("toggle_content", { instanceId, kind, filename }),
  remove: (instanceId: string, kind: string, filename: string) =>
    invoke<void>("delete_content", { instanceId, kind, filename }),
  import: (instanceId: string, kind: string, path: string) =>
    invoke<ContentFile>("import_content", { instanceId, kind, path }),
  dir: (instanceId: string, kind: string) =>
    invoke<string>("get_content_dir", { instanceId, kind }),
};

export const worlds = {
  list: (instanceId: string) => invoke<WorldInfo[]>("list_worlds", { instanceId }),
  remove: (instanceId: string, folder: string) =>
    invoke<void>("delete_world", { instanceId, folder }),
  backup: (instanceId: string, folder: string) =>
    invoke<string>("backup_world", { instanceId, folder }),
  import: (instanceId: string, path: string) =>
    invoke<WorldInfo>("import_world", { instanceId, path }),
  dir: (instanceId: string) => invoke<string>("get_worlds_dir", { instanceId }),
};


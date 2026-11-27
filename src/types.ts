export interface Settings {
  nickname: string;
  ramMb: number;
  javaPath: string | null;
  closeOnLaunch: boolean;
  concurrency: number;
  accent: string;
}

export interface Account {
  id: string;
  kind: string;
  name: string;
  uuid: string;
  xuid: string | null;
  createdAt: number;
}

export interface Accounts {
  accounts: Account[];
  activeId: string | null;
}

export interface DeviceCodeInfo {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
  message: string;
}

export interface MicrosoftPollResult {
  status: "pending" | "success" | "error";
  message: string;
  accounts: Accounts | null;
}

export interface Instance {
  id: string;
  name: string;
  versionId: string;
  loader: string;
  loaderVersion: string | null;
  createdAt: number;
  lastPlayed: number | null;
  installed: boolean;
  icon: string | null;
  ramMb: number | null;
  javaPath: string | null;
  jvmArgs: string | null;
}

export interface PackManifest {
  projectId: string;
  versionId: string;
  name: string;
  files: string[];
}

export interface VersionSummary {
  id: string;
  kind: string;
  releaseTime: string;
  url: string;
}

export interface FabricLoader {
  version: string;
  stable: boolean;
}

export interface ForgeVersion {
  version: string;
  recommended: boolean;
}

export interface JavaCheck {
  required: number;
  found: boolean;
}

export interface JavaInfo {
  path: string;
  version: string;
  major: number;
}

export interface Progress {
  instanceId: string;
  stage: string;
  current: number;
  total: number;
  message: string;
}

export interface LaunchInfo {
  pid: number;
  command: string;
}

export interface ModrinthHit {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  categories: string[];
  projectType: string;
  downloads: number;
  iconUrl: string | null;
  author: string | null;
  versions: string[];
}

export interface ModrinthSearchResult {
  hits: ModrinthHit[];
  total: number;
  offset: number;
  limit: number;
}

export interface ModrinthVersion {
  id: string;
  name: string;
  versionNumber: string;
  gameVersions: string[];
  loaders: string[];
  filename: string;
  size: number;
  datePublished: string;
}

export interface InstalledMod {
  filename: string;
  name: string;
  version: string;
  size: number;
  enabled: boolean;
  projectId: string | null;
}

export interface ContentFile {
  filename: string;
  name: string;
  size: number;
  enabled: boolean;
  modified: number;
}

export interface WorldInfo {
  folder: string;
  name: string;
  size: number;
  lastModified: number;
}

export interface InstanceStats {
  sizeBytes: number;
  mods: number;
  worlds: number;
}

export interface LatestRelease {
  tag: string;
  name: string;
  url: string;
}

export interface LauncherUpdate {
  available: boolean;
  current: string;
  latest: LatestRelease | null;
  dismissed: boolean;
}

export interface PackUpdateInfo {
  instanceId: string;
  projectId: string;
  currentVersionId: string;
  latestVersionId: string;
  latestVersionNumber: string;
  latestName: string;
  hasUpdate: boolean;
}

export interface DownloadTask {
  id: string;
  name: string;
  version: string;
  stage: string;
  current: number;
  total: number;
  status: "active" | "paused" | "done" | "error";
  message?: string;
}


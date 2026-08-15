import { memo, useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type PointerEvent } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import amdAdrenalinIcon from "./assets/software-icons/amd-adrenalin.png";
import chatGptIcon from "./assets/software-icons/chatgpt.png";
import leagueAkariIcon from "./assets/software-icons/leagueakari.png";
import quickClipboardIcon from "./assets/software-icons/quickclipboard.png";
import sTranslateIcon from "./assets/software-icons/stranslate.png";
import weGameIcon from "./assets/software-icons/wegame.png";
import wingetIcon from "./assets/software-icons/winget.png";
import "./App.css";

/*  data types  */

interface SoftwareAsset {
  name: string;
  browser_download_url: string;
  size: number;
}

interface SoftwareInfo {
  id: string;
  display_name: string;
  latest_version: string;
  release_url: string;
  published_at: string;
  portable: SoftwareAsset | null;
  install_kind: "portable" | "installer" | "download" | "store";
  source_kind: "github" | "official" | "store";
  ocr_install: boolean;
  silent_install_args: string;
  icon_path: string;
}

interface InstallPathSettings {
  default_base: string;
  current_base: string;
}

interface AppInstallPaths {
  install_dir: string;
  download_file: string;
  package_file: string;
  shortcut: string;
}

interface PackageCacheInfo {
  cached: boolean;
  path: string;
  size: number;
}

interface PackageLaunchResult {
  message: string;
  tracked: boolean;
}

interface WingetStatus {
  available: boolean;
  version: string;
  message: string;
}

interface WingetSearchResult {
  name: string;
  packageId: string;
  version: string;
}

interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  percent: number;
}

type InstallState = "downloading" | "paused" | "cancelling" | "cancelled" | "uninstalling" | "installing" | "checking" | "done" | "error" | "uninstall_failed";

interface InstallStatus {
  state: InstallState;
  percent: number;
  message: string;
}

interface AutomationStep {
  id: string;
  name: string;
  action?: "click" | "inputText" | "closeWindow";
  windowTitle: string;
  matchType: "text" | "color" | "point";
  matchValue: string;
  colorTolerance: number;
  click: boolean;
  enabled?: boolean;
  delayMs: number;
  delayUnit?: "ms" | "s";
  timeAfterStep?: boolean;
  lastMeasuredMs?: number;
  offsetX: number;
  offsetY: number;
  inputMode?: "installBase" | "custom";
  inputText?: string;
}

interface StepDraft {
  name: string;
  action: "click" | "inputText" | "closeWindow";
  windowTitle: string;
  matchType: "text" | "color" | "point";
  matchValue: string;
  colorTolerance: number;
  click: boolean;
  offsetX: number;
  offsetY: number;
  inputMode: "installBase" | "custom";
  inputText: string;
}

interface VisualTargetResult {
  success: boolean;
  raw_screen_x?: number | null;
  raw_screen_y?: number | null;
  offset_x?: number | null;
  offset_y?: number | null;
  screen_x?: number | null;
  screen_y?: number | null;
  window_left?: number | null;
  window_top?: number | null;
  window_width?: number | null;
  window_height?: number | null;
  window_title?: string | null;
  detail?: string | null;
  preview_image?: string | null;
  message: string;
}

interface PickedScreenColor {
  hex: string;
  r: number;
  g: number;
  b: number;
  screen_x: number;
  screen_y: number;
}

interface WeGameInstallResult {
  success: boolean;
  message: string;
  exit_code?: number | null;
  pid?: number | null;
  installed: boolean;
}

interface AutomationTemplate {
  id: string;
  name: string;
  steps: AutomationStep[];
  updatedAt: number;
}

interface CustomSourceForm {
  id: string;
  display_name: string;
  source_kind: "github" | "direct";
  repo: string;
  asset_match: string;
  exe_match: string;
  download_url: string;
  page_url: string;
  version: string;
  file_name: string;
  silent_install_args: string;
  icon_path: string;
}

interface DownloadCandidate {
  url: string;
  file_name: string;
  version: string;
  size: number;
  source_page: string;
  matcher: string;
  score: number;
  display_name: string;
}

interface CustomSourceScanTarget {
  source_kind: CustomSourceForm["source_kind"];
  source_url: string;
  repo: string;
}

const DEFAULT_CUSTOM_SOURCE_FORM: CustomSourceForm = {
  id: "",
  display_name: "",
  source_kind: "direct",
  repo: "",
  asset_match: "",
  exe_match: "",
  download_url: "",
  page_url: "",
  version: "",
  file_name: "",
  silent_install_args: "",
  icon_path: "",
};

const BUILT_IN_SOFTWARE_IDS = new Set(["stranslate", "quickclipboard", "leagueakari", "wegame", "amd-adrenalin", "chatgpt"]);

const BUILT_IN_ICON_SOURCES: Record<string, string> = {
  stranslate: sTranslateIcon,
  quickclipboard: quickClipboardIcon,
  leagueakari: leagueAkariIcon,
  wegame: weGameIcon,
  "amd-adrenalin": amdAdrenalinIcon,
  chatgpt: chatGptIcon,
};

function slugifySourceId(name: string) {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");
  return slug || "source";
}

function generateCustomSourceId(name: string, existingIds: Iterable<string>) {
  const used = new Set(existingIds);
  const base = slugifySourceId(name);
  let candidate = base;
  let index = 2;
  while (used.has(candidate)) {
    candidate = `${base}-${index}`;
    index += 1;
  }
  return candidate;
}

function titleFromFileName(fileName: string) {
  const raw = fileName.split(/[?#]/)[0].split(/[\\/]/).pop() ?? fileName;
  const stem = raw.replace(/\.[a-z0-9]{2,10}$/i, "");
  const words = stem
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .split(/[^a-zA-Z0-9]+/)
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((part) => !/^\d+(\.\d+)*$/.test(part))
    .filter((part) => !/^(setup|installer|install|download|release|stable|latest|x64|x86|win|windows)$/i.test(part));
  if (!words.length) return "";
  return words
    .map((word) => {
      const lower = word.toLowerCase();
      if (word.length <= 3 && word === word.toUpperCase()) return word;
      return lower.charAt(0).toUpperCase() + lower.slice(1);
    })
    .join(" ");
}

function fileNameFromUrl(url: string) {
  try {
    const parsed = new URL(url);
    return decodeURIComponent(parsed.pathname.split("/").pop() ?? "");
  } catch {
    return decodeURIComponent(url.split(/[?#]/)[0].split("/").pop() ?? "");
  }
}

function parseGithubRepo(input: string) {
  const trimmed = input.trim().replace(/\/+$/, "");
  const withoutHost = trimmed
    .replace(/^https?:\/\/github\.com\//i, "")
    .replace(/^github\.com\//i, "");
  const [owner, repo] = withoutHost.split("/").filter(Boolean);
  if (!owner || !repo || owner.includes(":") || repo.includes(":")) return "";
  return `${owner.replace(/\.git$/i, "")}/${repo.replace(/\.git$/i, "")}`;
}

function titleFromRepo(repo: string) {
  const name = repo.split("/").pop() ?? "";
  return titleFromFileName(name) || name;
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, ms));
}

async function loadRunnableAutomationSteps() {
  const steps = await invoke<AutomationStep[]>("get_automation_steps_cmd");
  return steps.map(normalizeStep).filter((step) => step.enabled !== false);
}

type NavId = "library" | "packages" | "winget" | "automation" | "settings";

type ManualWingetAction = "search" | "install" | "upgrade";
type CardDropPosition = "before" | "after";

const WINGET_MANUAL_ACTIONS: Array<{
  id: ManualWingetAction;
  label: string;
}> = [
  { id: "search", label: "查找" },
  { id: "install", label: "安装" },
  { id: "upgrade", label: "更新" },
];

interface WingetUserCard {
  id: string;
  title: string;
  command: string;
  categoryId: string | null;
}

function normalizeHttpLink(input: string) {
  const value = input.trim();
  if (!value || /^https?:\/\//i.test(value)) return value;
  return `https://${value}`;
}

function githubRepoFromLink(input: string) {
  const trimmed = input.trim();
  const looksLikeGithubUrl = /^(?:https?:\/\/)?github\.com\//i.test(trimmed);
  const looksLikeRepo = /^[a-z0-9_.-]+\/[a-z0-9_.-]+\/?$/i.test(trimmed);
  return looksLikeGithubUrl || looksLikeRepo ? parseGithubRepo(trimmed) : "";
}

interface WingetUserCategory {
  id: string;
  name: string;
}

const WINGET_USER_CARDS_STORAGE_KEY = "software-manager.winget-user-cards.v1";
const WINGET_USER_CATEGORIES_STORAGE_KEY = "software-manager.winget-user-categories.v1";

function loadWingetUserCards(): WingetUserCard[] {
  try {
    const raw = window.localStorage.getItem(WINGET_USER_CARDS_STORAGE_KEY);
    if (!raw) return [];
    const cards: unknown = JSON.parse(raw);
    if (!Array.isArray(cards)) return [];
    return cards.flatMap((card): WingetUserCard[] => {
      if (!card || typeof card !== "object") return [];
      const { id, title, command, categoryId } = card as Record<string, unknown>;
      if (typeof id !== "string" || typeof title !== "string" || typeof command !== "string") return [];
      return [{ id, title, command, categoryId: typeof categoryId === "string" ? categoryId : null }];
    });
  } catch {
    return [];
  }
}

function loadWingetUserCategories(): WingetUserCategory[] {
  try {
    const raw = window.localStorage.getItem(WINGET_USER_CATEGORIES_STORAGE_KEY);
    if (!raw) return [];
    const categories: unknown = JSON.parse(raw);
    if (!Array.isArray(categories)) return [];
    return categories.flatMap((category): WingetUserCategory[] => {
      if (!category || typeof category !== "object") return [];
      const { id, name } = category as Record<string, unknown>;
      if (typeof id !== "string" || typeof name !== "string" || !name.trim()) return [];
      return [{ id, name: name.trim() }];
    });
  } catch {
    return [];
  }
}

function createWingetUserCardId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `winget-card-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function createWingetUserCategoryId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `winget-category-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}


function isLikelyWingetPackageId(value: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value) && (value.includes(".") || /^[A-Z0-9]{10,}$/.test(value));
}

function isFullWingetCommand(value: string): boolean {
  return /^winget(?:\.exe)?(?:\s|$)/i.test(value.trim());
}

function getFullWingetCommandAction(value: string): ManualWingetAction | null {
  const action = value.trim().match(/^winget(?:\.exe)?\s+(search|install|upgrade)\b/i)?.[1]?.toLowerCase();
  return action === "search" || action === "install" || action === "upgrade" ? action : null;
}

function getFullWingetCommandTarget(value: string): string {
  const match = value.match(/(?:^|\s)--(?:id|name)\s+(?:"([^"]+)"|'([^']+)'|([^\s`]+))/i);
  return match?.[1] ?? match?.[2] ?? match?.[3] ?? "";
}

function getWingetSearchTarget(value: string): string {
  const input = value.trim();
  if (!input) return "";

  const optionMatch = input.match(/(?:^|\s)--(?:id|name|query|moniker|tag|command)(?:=|\s+)(?:"([^"]+)"|'([^']+)'|([^\s`]+))/i);
  if (optionMatch) return optionMatch[1] ?? optionMatch[2] ?? optionMatch[3] ?? "";
  if (!isFullWingetCommand(input)) return input;

  const positional = input
    .replace(/^winget(?:\.exe)?\s+(?:search|install|upgrade|uninstall)\b/i, "")
    .trim()
    .match(/^(?:"([^"]+)"|'([^']+)'|([^\s`]+))/);
  return positional?.[1] ?? positional?.[2] ?? positional?.[3] ?? "";
}

function replaceFullWingetCommandTarget(command: string, packageId: string): string {
  const target = packageId.trim();
  if (!target || !isFullWingetCommand(command)) return command;

  const optionTarget = /(--(?:id|name)(?:=|\s+))(?:"[^"]+"|'[^']+'|[^\s`]+)/i;
  if (optionTarget.test(command)) {
    return command.replace(optionTarget, `$1${target}`);
  }

  const positionalTarget = /^(winget(?:\.exe)?\s+(?:install|upgrade|uninstall)\s+)(?:"[^"]+"|'[^']+'|[^\s`]+)/i;
  if (positionalTarget.test(command)) {
    return command.replace(positionalTarget, `$1--id ${target} -e`);
  }

  return command;
}

function makeManualWingetCommand(action: ManualWingetAction, value: string): string {
  const input = value.trim();
  if (!input) return "";
  if (isFullWingetCommand(input)) return input;
  if (action === "search") return `winget search --query "${input}"`;
  const target = isLikelyWingetPackageId(input) ? `--id ${input} -e` : `--name "${input}" -e`;
  return `winget ${action} ${target}`;
}

function makeWingetCardTitle(action: ManualWingetAction, value: string): string {
  const target = value.trim();
  if (isFullWingetCommand(target)) {
    const commandAction = getFullWingetCommandAction(target);
    const commandTarget = getFullWingetCommandTarget(target);
    if (commandAction === "search") return commandTarget ? `查找 ${commandTarget}` : "自定义 Winget 查询";
    if (commandAction === "install") return commandTarget ? `安装 ${commandTarget}` : "自定义 Winget 安装";
    if (commandAction === "upgrade") return commandTarget ? `更新 ${commandTarget}` : "自定义 Winget 更新";
    return "自定义 Winget 命令";
  }
  if (action === "search") return target ? `查找 ${target}` : "查找软件";
  if (action === "install") return target ? `安装 ${target}` : "安装软件";
  return target ? `更新 ${target}` : "更新软件";
}

interface WingetCardLayoutSlot {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface WingetCategoryDragCandidate {
  id: string;
  index: number;
  element: HTMLElement;
  startX: number;
  startY: number;
}

function captureWingetCardLayouts(grid: HTMLElement): WingetCardLayoutSlot[] {
  return Array.from(grid.querySelectorAll<HTMLElement>("[data-winget-card-index]")).map((card) => {
    const rect = card.getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  });
}

function captureWingetCategoryLayouts(list: HTMLElement): WingetCardLayoutSlot[] {
  return Array.from(list.querySelectorAll<HTMLElement>("[data-winget-category-index]")).map((category) => {
    const rect = category.getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  });
}

function resolveWingetCardDropIndex(clientX: number, clientY: number, layouts: WingetCardLayoutSlot[]): number {
  if (layouts.length === 0) return 0;
  let closestIndex = 0;
  let closestDistance = Number.POSITIVE_INFINITY;
  layouts.forEach((layout, index) => {
    const centerX = layout.left + layout.width / 2;
    const centerY = layout.top + layout.height / 2;
    const distance = (clientX - centerX) ** 2 + (clientY - centerY) ** 2;
    if (distance < closestDistance) {
      closestDistance = distance;
      closestIndex = index;
    }
  });
  return closestIndex;
}

function getWingetCardShift(
  index: number,
  from: number,
  over: number,
  layouts: WingetCardLayoutSlot[],
): { x: number; y: number } {
  if (from === over) return { x: 0, y: 0 };
  const current = layouts[index];
  if (!current) return { x: 0, y: 0 };
  const target = from < over && index > from && index <= over
    ? layouts[index - 1]
    : from > over && index >= over && index < from
      ? layouts[index + 1]
      : null;
  if (!target) return { x: 0, y: 0 };
  return { x: target.left - current.left, y: target.top - current.top };
}

const WINGET_CARD_DRAG_SETTLE_MS = 300;
const WINGET_CATEGORY_DRAG_START_DISTANCE = 6;

const WingetPage = memo(function WingetPage({
  status,
  checking,
  onRefresh,
  onOpenTerminal,
  onSearch,
}: {
  status: WingetStatus | null;
  checking: boolean;
  onRefresh: () => Promise<void>;
  onOpenTerminal: () => Promise<void>;
  onSearch: (query: string) => Promise<WingetSearchResult[]>;
}) {
  const available = status?.available === true;
  const statusLabel = checking ? "检测中" : status ? (available ? "可用" : "未检测到") : "待检测";
  const [manualAction, setManualAction] = useState<ManualWingetAction>("search");
  const [manualValue, setManualValue] = useState("");
  const [searchResults, setSearchResults] = useState<WingetSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchMessage, setSearchMessage] = useState("");
  const [userCards, setUserCards] = useState<WingetUserCard[]>(loadWingetUserCards);
  const [userCategories, setUserCategories] = useState<WingetUserCategory[]>(loadWingetUserCategories);
  const [activeCategoryId, setActiveCategoryId] = useState("all");
  const [selectedCardIds, setSelectedCardIds] = useState<string[]>([]);
  const [draggingCardId, setDraggingCardId] = useState<string | null>(null);
  const [cardDragFromIdx, setCardDragFromIdx] = useState<number | null>(null);
  const [cardDragOverIdx, setCardDragOverIdx] = useState<number | null>(null);
  const [cardDragPointer, setCardDragPointer] = useState({ x: 0, y: 0 });
  const [cardDragGhostSize, setCardDragGhostSize] = useState({ width: 0, height: 0 });
  const [cardDragSettling, setCardDragSettling] = useState(false);
  const [draggingCategoryId, setDraggingCategoryId] = useState<string | null>(null);
  const [categoryDragPending, setCategoryDragPending] = useState(false);
  const [categoryDragFromIdx, setCategoryDragFromIdx] = useState<number | null>(null);
  const [categoryDragOverIdx, setCategoryDragOverIdx] = useState<number | null>(null);
  const [categoryDragPointer, setCategoryDragPointer] = useState({ x: 0, y: 0 });
  const [categoryDragGhostSize, setCategoryDragGhostSize] = useState({ width: 0, height: 0 });
  const [addingCategory, setAddingCategory] = useState(false);
  const [draftCategoryName, setDraftCategoryName] = useState("");
  const [categoryError, setCategoryError] = useState("");
  const [addingCard, setAddingCard] = useState(false);
  const [draftTitle, setDraftTitle] = useState("");
  const [draftCommand, setDraftCommand] = useState("");
  const [draftCategoryId, setDraftCategoryId] = useState("");
  const [cardError, setCardError] = useState("");
  const [editingCardId, setEditingCardId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editCommand, setEditCommand] = useState("");
  const [editCategoryId, setEditCategoryId] = useState("");
  const [editCardError, setEditCardError] = useState("");
  const [copyState, setCopyState] = useState<{ id: string; ok: boolean } | null>(null);
  const [saveState, setSaveState] = useState<"saved" | "exists" | null>(null);
  const [combineState, setCombineState] = useState<"saved" | null>(null);
  const copyTimer = useRef<number | null>(null);
  const saveTimer = useRef<number | null>(null);
  const combineTimer = useRef<number | null>(null);
  const cardDragFromIdxRef = useRef<number | null>(null);
  const cardDragOverIdxRef = useRef<number | null>(null);
  const cardDragOffsetRef = useRef({ x: 24, y: 22 });
  const cardDragRafRef = useRef<number | null>(null);
  const cardDragSettleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cardDragLayoutsRef = useRef<WingetCardLayoutSlot[] | null>(null);
  const cardDragOrderRef = useRef<string[] | null>(null);
  const categoryDragFromIdxRef = useRef<number | null>(null);
  const categoryDragOverIdxRef = useRef<number | null>(null);
  const categoryDragOffsetRef = useRef({ x: 16, y: 15 });
  const categoryDragRafRef = useRef<number | null>(null);
  const categoryDragLayoutsRef = useRef<WingetCardLayoutSlot[] | null>(null);
  const categoryDragOrderRef = useRef<string[] | null>(null);
  const categoryDragCandidateRef = useRef<WingetCategoryDragCandidate | null>(null);
  const categoryDragClickSuppressRef = useRef(false);
  const categoryDragClickResetTimerRef = useRef<number | null>(null);

  useEffect(() => () => {
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    if (combineTimer.current !== null) window.clearTimeout(combineTimer.current);
    if (cardDragRafRef.current !== null) cancelAnimationFrame(cardDragRafRef.current);
    if (cardDragSettleTimerRef.current !== null) window.clearTimeout(cardDragSettleTimerRef.current);
    if (categoryDragRafRef.current !== null) cancelAnimationFrame(categoryDragRafRef.current);
    if (categoryDragClickResetTimerRef.current !== null) window.clearTimeout(categoryDragClickResetTimerRef.current);
  }, []);

  useEffect(() => {
    try {
      window.localStorage.setItem(WINGET_USER_CARDS_STORAGE_KEY, JSON.stringify(userCards));
    } catch {
      // The page stays usable even if browser storage is unavailable.
    }
  }, [userCards]);

  useEffect(() => {
    try {
      window.localStorage.setItem(WINGET_USER_CATEGORIES_STORAGE_KEY, JSON.stringify(userCategories));
    } catch {
      // The page stays usable even if browser storage is unavailable.
    }
  }, [userCategories]);

  const manualCommand = useMemo(
    () => makeManualWingetCommand(manualAction, manualValue),
    [manualAction, manualValue],
  );
  const canSaveManualCommand = Boolean(manualValue.trim());
  const visibleUserCards = useMemo(() => {
    if (activeCategoryId === "all") return userCards;
    if (activeCategoryId === "uncategorized") return userCards.filter((card) => !card.categoryId);
    return userCards.filter((card) => card.categoryId === activeCategoryId);
  }, [activeCategoryId, userCards]);
  const draggedUserCard = draggingCardId ? userCards.find((card) => card.id === draggingCardId) ?? null : null;
  const draggedUserCategory = draggingCategoryId ? userCategories.find((category) => category.id === draggingCategoryId) ?? null : null;
  const isCardDragActive = draggingCardId !== null || cardDragSettling;
  const selectedCards = useMemo(
    () => userCards.filter((card) => selectedCardIds.includes(card.id)),
    [selectedCardIds, userCards],
  );
  const combinedCommand = useMemo(
    () => selectedCards.map((card) => card.command.trim()).filter(Boolean).join("\n\n"),
    [selectedCards],
  );

  const copyCommand = useCallback(async (id: string, command: string) => {
    if (!command.trim()) return;
    if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
    try {
      await navigator.clipboard.writeText(command);
      setCopyState({ id, ok: true });
    } catch {
      setCopyState({ id, ok: false });
    }
    copyTimer.current = window.setTimeout(() => setCopyState(null), 1400);
  }, []);

  const selectCategory = useCallback((categoryId: string) => {
    setActiveCategoryId(categoryId);
    setSelectedCardIds([]);
  }, []);

  const selectCategoryFromFilter = useCallback((categoryId: string) => {
    if (categoryDragClickSuppressRef.current) {
      categoryDragClickSuppressRef.current = false;
      return;
    }
    selectCategory(categoryId);
  }, [selectCategory]);

  const startAddingCategory = useCallback(() => {
    setAddingCard(false);
    setDraftTitle("");
    setDraftCommand("");
    setDraftCategoryId("");
    setCardError("");
    setEditingCardId(null);
    setEditTitle("");
    setEditCommand("");
    setEditCategoryId("");
    setEditCardError("");
    setCategoryError("");
    setAddingCategory(true);
  }, []);

  const cancelAddingCategory = useCallback(() => {
    setAddingCategory(false);
    setDraftCategoryName("");
    setCategoryError("");
  }, []);

  const addUserCategory = useCallback((event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = draftCategoryName.trim();
    if (!name) {
      setCategoryError("分类名称要填写。");
      return;
    }
    if (userCategories.some((category) => category.name.toLocaleLowerCase() === name.toLocaleLowerCase())) {
      setCategoryError("这个分类已经有了。");
      return;
    }
    const category = { id: createWingetUserCategoryId(), name };
    setUserCategories((current) => [...current, category]);
    setActiveCategoryId(category.id);
    setSelectedCardIds([]);
    cancelAddingCategory();
  }, [cancelAddingCategory, draftCategoryName, userCategories]);

  const deleteUserCategory = useCallback((category: WingetUserCategory) => {
    if (!window.confirm(`删除分类「${category.name}」？其中的卡片会变为未分类。`)) return;
    setUserCategories((current) => current.filter((item) => item.id !== category.id));
    setUserCards((current) => current.map((card) => (
      card.categoryId === category.id ? { ...card, categoryId: null } : card
    )));
    setActiveCategoryId((current) => current === category.id ? "all" : current);
    setDraftCategoryId((current) => current === category.id ? "" : current);
    setEditCategoryId((current) => current === category.id ? "" : current);
  }, []);

  const startAddingCard = useCallback(() => {
    setAddingCategory(false);
    setDraftCategoryName("");
    setCategoryError("");
    setCardError("");
    setEditingCardId(null);
    setEditTitle("");
    setEditCommand("");
    setEditCategoryId("");
    setEditCardError("");
    setDraftCategoryId(userCategories.some((category) => category.id === activeCategoryId) ? activeCategoryId : "");
    setAddingCard(true);
  }, [activeCategoryId, userCategories]);

  const cancelAddingCard = useCallback(() => {
    setAddingCard(false);
    setDraftTitle("");
    setDraftCommand("");
    setDraftCategoryId("");
    setCardError("");
  }, []);

  const startEditingCard = useCallback((card: WingetUserCard) => {
    setAddingCategory(false);
    setDraftCategoryName("");
    setCategoryError("");
    setAddingCard(false);
    setDraftTitle("");
    setDraftCommand("");
    setDraftCategoryId("");
    setCardError("");
    setEditingCardId(card.id);
    setEditTitle(card.title);
    setEditCommand(card.command);
    setEditCategoryId(card.categoryId ?? "");
    setEditCardError("");
  }, []);

  const cancelEditingCard = useCallback(() => {
    setEditingCardId(null);
    setEditTitle("");
    setEditCommand("");
    setEditCategoryId("");
    setEditCardError("");
  }, []);

  const addUserCard = useCallback((event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const title = draftTitle.trim();
    const command = draftCommand.trim();
    if (!title || !command) {
      setCardError("名称和命令都要填写。");
      return;
    }
    setUserCards((current) => [...current, {
      id: createWingetUserCardId(),
      title,
      command,
      categoryId: draftCategoryId || null,
    }]);
    cancelAddingCard();
  }, [cancelAddingCard, draftCategoryId, draftCommand, draftTitle]);

  const updateUserCard = useCallback((event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const title = editTitle.trim();
    const command = editCommand.trim();
    if (!editingCardId || !title || !command) {
      setEditCardError("名称和命令都要填写。");
      return;
    }
    setUserCards((current) => current.map((card) => (
      card.id === editingCardId ? { ...card, title, command, categoryId: editCategoryId || null } : card
    )));
    cancelEditingCard();
  }, [cancelEditingCard, editCategoryId, editCommand, editTitle, editingCardId]);

  const deleteUserCard = useCallback((card: WingetUserCard) => {
    if (!window.confirm(`删除「${card.title}」？`)) return;
    setUserCards((current) => current.filter((item) => item.id !== card.id));
    setSelectedCardIds((current) => current.filter((id) => id !== card.id));
    if (editingCardId === card.id) cancelEditingCard();
  }, [cancelEditingCard, editingCardId]);

  const toggleUserCardSelection = useCallback((id: string) => {
    setSelectedCardIds((current) => (
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id]
    ));
  }, []);

  const moveUserCard = useCallback((sourceId: string, targetId: string, position: CardDropPosition) => {
    if (sourceId === targetId) return;
    setUserCards((current) => {
      const sourceIndex = current.findIndex((card) => card.id === sourceId);
      if (sourceIndex < 0) return current;
      const next = [...current];
      const [sourceCard] = next.splice(sourceIndex, 1);
      const targetIndex = next.findIndex((card) => card.id === targetId);
      if (targetIndex < 0 || !sourceCard) return current;
      next.splice(position === "after" ? targetIndex + 1 : targetIndex, 0, sourceCard);
      return next;
    });
  }, []);

  const moveUserCategory = useCallback((sourceId: string, targetId: string, position: CardDropPosition) => {
    if (sourceId === targetId) return;
    setUserCategories((current) => {
      const sourceIndex = current.findIndex((category) => category.id === sourceId);
      if (sourceIndex < 0) return current;
      const next = [...current];
      const [sourceCategory] = next.splice(sourceIndex, 1);
      const targetIndex = next.findIndex((category) => category.id === targetId);
      if (targetIndex < 0 || !sourceCategory) return current;
      next.splice(position === "after" ? targetIndex + 1 : targetIndex, 0, sourceCategory);
      return next;
    });
  }, []);

  const clearUserCardDragState = useCallback(() => {
    if (cardDragRafRef.current !== null) {
      cancelAnimationFrame(cardDragRafRef.current);
      cardDragRafRef.current = null;
    }
    if (cardDragSettleTimerRef.current !== null) {
      window.clearTimeout(cardDragSettleTimerRef.current);
      cardDragSettleTimerRef.current = null;
    }
    cardDragFromIdxRef.current = null;
    cardDragOverIdxRef.current = null;
    cardDragLayoutsRef.current = null;
    cardDragOrderRef.current = null;
    setDraggingCardId(null);
    setCardDragFromIdx(null);
    setCardDragOverIdx(null);
    setCardDragSettling(false);
  }, []);

  const clearUserCategoryDragState = useCallback(() => {
    if (categoryDragRafRef.current !== null) {
      cancelAnimationFrame(categoryDragRafRef.current);
      categoryDragRafRef.current = null;
    }
    categoryDragFromIdxRef.current = null;
    categoryDragOverIdxRef.current = null;
    categoryDragLayoutsRef.current = null;
    categoryDragOrderRef.current = null;
    categoryDragCandidateRef.current = null;
    setDraggingCategoryId(null);
    setCategoryDragPending(false);
    setCategoryDragFromIdx(null);
    setCategoryDragOverIdx(null);
  }, []);

  const beginUserCardDrag = useCallback((event: PointerEvent<HTMLButtonElement>, id: string, index: number) => {
    if (event.button !== 0) return;
    if (cardDragSettleTimerRef.current !== null) clearUserCardDragState();
    event.preventDefault();
    event.stopPropagation();
    const cardElement = event.currentTarget.closest<HTMLElement>("[data-winget-card-id]");
    if (!cardElement) return;
    const rect = cardElement.getBoundingClientRect();
    const grid = cardElement.closest<HTMLElement>(".winget-user-card-grid");
    cardDragOffsetRef.current = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    setCardDragGhostSize({ width: rect.width, height: rect.height });
    if (grid) {
      cardDragLayoutsRef.current = captureWingetCardLayouts(grid);
      cardDragOrderRef.current = Array.from(grid.querySelectorAll<HTMLElement>("[data-winget-card-id]"))
        .map((card) => card.dataset.wingetCardId ?? "")
        .filter(Boolean);
    }
    cardDragFromIdxRef.current = index;
    cardDragOverIdxRef.current = index;
    setDraggingCardId(id);
    setCardDragFromIdx(index);
    setCardDragOverIdx(index);
    setCardDragPointer({ x: event.clientX, y: event.clientY });
  }, [clearUserCardDragState]);

  const activateUserCategoryDrag = useCallback((candidate: WingetCategoryDragCandidate, clientX: number, clientY: number) => {
    const rect = candidate.element.getBoundingClientRect();
    const list = candidate.element.closest<HTMLElement>(".winget-category-list");
    categoryDragOffsetRef.current = { x: clientX - rect.left, y: clientY - rect.top };
    setCategoryDragGhostSize({ width: rect.width, height: rect.height });
    if (list) {
      categoryDragLayoutsRef.current = captureWingetCategoryLayouts(list);
      categoryDragOrderRef.current = Array.from(list.querySelectorAll<HTMLElement>("[data-winget-category-id]"))
        .map((category) => category.dataset.wingetCategoryId ?? "")
        .filter(Boolean);
    }
    categoryDragFromIdxRef.current = candidate.index;
    categoryDragOverIdxRef.current = candidate.index;
    setDraggingCategoryId(candidate.id);
    setCategoryDragFromIdx(candidate.index);
    setCategoryDragOverIdx(candidate.index);
    setCategoryDragPointer({ x: clientX, y: clientY });
  }, []);

  const beginUserCategoryDrag = useCallback((event: PointerEvent<HTMLButtonElement>, id: string, index: number) => {
    if (event.button !== 0 || userCategories.length < 2 || draggingCategoryId) return;
    const categoryElement = event.currentTarget.closest<HTMLElement>("[data-winget-category-id]");
    if (!categoryElement) return;
    categoryDragCandidateRef.current = {
      id,
      index,
      element: categoryElement,
      startX: event.clientX,
      startY: event.clientY,
    };
    setCategoryDragPending(true);
  }, [draggingCategoryId, userCategories.length]);

  useEffect(() => {
    if (!categoryDragPending) return;

    const cancelPendingDrag = () => {
      categoryDragCandidateRef.current = null;
      setCategoryDragPending(false);
    };

    const onMove = (event: globalThis.PointerEvent) => {
      const candidate = categoryDragCandidateRef.current;
      if (!candidate) return;
      const distance = Math.hypot(event.clientX - candidate.startX, event.clientY - candidate.startY);
      if (distance < WINGET_CATEGORY_DRAG_START_DISTANCE) return;

      event.preventDefault();
      categoryDragClickSuppressRef.current = true;
      categoryDragCandidateRef.current = null;
      setCategoryDragPending(false);
      activateUserCategoryDrag(candidate, event.clientX, event.clientY);
    };

    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", cancelPendingDrag);
    window.addEventListener("pointercancel", cancelPendingDrag);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", cancelPendingDrag);
      window.removeEventListener("pointercancel", cancelPendingDrag);
    };
  }, [activateUserCategoryDrag, categoryDragPending]);

  useEffect(() => {
    if (!draggingCardId) return;

    const onMove = (event: globalThis.PointerEvent) => {
      if (cardDragFromIdxRef.current === null) return;
      const x = event.clientX;
      const y = event.clientY;
      if (cardDragRafRef.current !== null) cancelAnimationFrame(cardDragRafRef.current);
      cardDragRafRef.current = requestAnimationFrame(() => {
        setCardDragPointer({ x, y });
        const layouts = cardDragLayoutsRef.current;
        if (layouts) {
          const index = resolveWingetCardDropIndex(x, y, layouts);
          if (index !== cardDragOverIdxRef.current) {
            cardDragOverIdxRef.current = index;
            setCardDragOverIdx(index);
          }
        }
        cardDragRafRef.current = null;
      });
    };

    const finish = () => {
      if (cardDragRafRef.current !== null) {
        cancelAnimationFrame(cardDragRafRef.current);
        cardDragRafRef.current = null;
      }
      const from = cardDragFromIdxRef.current;
      const over = cardDragOverIdxRef.current;
      if (from !== null && over !== null && from !== over) {
        const order = cardDragOrderRef.current;
        const sourceId = order?.[from];
        const targetId = order?.[over];
        cardDragFromIdxRef.current = null;
        cardDragOverIdxRef.current = null;
        cardDragLayoutsRef.current = null;
        cardDragOrderRef.current = null;
        setDraggingCardId(null);
        setCardDragFromIdx(null);
        setCardDragOverIdx(null);
        setCardDragSettling(false);
        if (sourceId && targetId) moveUserCard(sourceId, targetId, from < over ? "after" : "before");
        return;
      }
      if (from !== null) {
        document.body.classList.remove("is-winget-card-dragging");
        document.body.style.userSelect = "";
        cardDragOverIdxRef.current = from;
        setCardDragOverIdx(from);
        setCardDragSettling(true);
        cardDragSettleTimerRef.current = window.setTimeout(() => {
          clearUserCardDragState();
        }, WINGET_CARD_DRAG_SETTLE_MS);
        return;
      }
      clearUserCardDragState();
    };

    document.body.style.userSelect = "none";
    document.body.classList.add("is-winget-card-dragging");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    return () => {
      document.body.style.userSelect = "";
      document.body.classList.remove("is-winget-card-dragging");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }, [clearUserCardDragState, draggingCardId, moveUserCard]);

  useEffect(() => {
    if (!draggingCategoryId) return;

    const onMove = (event: globalThis.PointerEvent) => {
      if (categoryDragFromIdxRef.current === null) return;
      const x = event.clientX;
      const y = event.clientY;
      if (categoryDragRafRef.current !== null) cancelAnimationFrame(categoryDragRafRef.current);
      categoryDragRafRef.current = requestAnimationFrame(() => {
        setCategoryDragPointer({ x, y });
        const layouts = categoryDragLayoutsRef.current;
        if (layouts) {
          const index = resolveWingetCardDropIndex(x, y, layouts);
          if (index !== categoryDragOverIdxRef.current) {
            categoryDragOverIdxRef.current = index;
            setCategoryDragOverIdx(index);
          }
        }
        categoryDragRafRef.current = null;
      });
    };

    const finish = () => {
      if (categoryDragRafRef.current !== null) {
        cancelAnimationFrame(categoryDragRafRef.current);
        categoryDragRafRef.current = null;
      }
      const from = categoryDragFromIdxRef.current;
      const over = categoryDragOverIdxRef.current;
      const order = categoryDragOrderRef.current;
      const sourceId = from === null ? undefined : order?.[from];
      const targetId = over === null ? undefined : order?.[over];
      if (from !== null && over !== null && from !== over && sourceId && targetId) {
        moveUserCategory(sourceId, targetId, from < over ? "after" : "before");
      }
      categoryDragClickSuppressRef.current = true;
      if (categoryDragClickResetTimerRef.current !== null) {
        window.clearTimeout(categoryDragClickResetTimerRef.current);
      }
      categoryDragClickResetTimerRef.current = window.setTimeout(() => {
        categoryDragClickSuppressRef.current = false;
        categoryDragClickResetTimerRef.current = null;
      }, 0);
      clearUserCategoryDragState();
    };

    document.body.style.userSelect = "none";
    document.body.classList.add("is-winget-category-dragging");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    return () => {
      document.body.style.userSelect = "";
      document.body.classList.remove("is-winget-category-dragging");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }, [clearUserCategoryDragState, draggingCategoryId, moveUserCategory]);

  const saveSelectedCardsAsCard = useCallback(() => {
    if (!combinedCommand || selectedCards.length === 0) return;
    const selectedCategoryIds = new Set(selectedCards.map((card) => card.categoryId ?? ""));
    const categoryId = selectedCategoryIds.size === 1 ? [...selectedCategoryIds][0] || null : null;
    if (combineTimer.current !== null) window.clearTimeout(combineTimer.current);
    setUserCards((current) => [
      ...current,
      {
        id: createWingetUserCardId(),
        title: `组合命令 ${selectedCards.length} 条`,
        command: combinedCommand,
        categoryId,
      },
    ]);
    setSelectedCardIds([]);
    setCombineState("saved");
    combineTimer.current = window.setTimeout(() => setCombineState(null), 1400);
  }, [combinedCommand, selectedCards.length]);

  const chooseManualAction = useCallback((action: ManualWingetAction) => {
    setManualAction(action);
    if (action !== "search") {
      setSearchResults([]);
      setSearchMessage("");
    }
  }, []);

  const changeManualValue = useCallback((value: string) => {
    setManualValue(value);
    const action = getFullWingetCommandAction(value);
    if (action) setManualAction(action);
    setSearchResults([]);
    setSearchMessage("");
  }, []);

  const searchPackages = useCallback(async () => {
    const query = getWingetSearchTarget(manualValue);
    if (!query) return;
    setSearching(true);
    setSearchResults([]);
    setSearchMessage("");
    try {
      const results = await onSearch(query);
      setSearchResults(results);
      if (results.length === 0) setSearchMessage("没有找到匹配的软件。");
    } catch (error) {
      setSearchMessage(String(error));
    } finally {
      setSearching(false);
    }
  }, [manualValue, onSearch]);

  const runManualAction = useCallback((action: ManualWingetAction) => {
    chooseManualAction(action);
    if (action === "search") void searchPackages();
  }, [chooseManualAction, searchPackages]);

  const useSearchResult = useCallback((result: WingetSearchResult) => {
    setManualAction("install");
    setManualValue((current) => {
      const updated = replaceFullWingetCommandTarget(current, result.packageId);
      return updated === current ? result.packageId : updated;
    });
    setSearchResults([]);
    setSearchMessage("");
  }, []);

  const saveCurrentCommandAsCard = useCallback(() => {
    if (!canSaveManualCommand) return;
    const command = manualCommand.trim();
    if (!command) return;
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    if (userCards.some((card) => card.command === command)) {
      setSaveState("exists");
    } else {
      const categoryId = userCategories.some((category) => category.id === activeCategoryId) ? activeCategoryId : null;
      setUserCards((current) => [
        ...current,
        { id: createWingetUserCardId(), title: makeWingetCardTitle(manualAction, manualValue), command, categoryId },
      ]);
      setSaveState("saved");
    }
    saveTimer.current = window.setTimeout(() => setSaveState(null), 1400);
  }, [activeCategoryId, canSaveManualCommand, manualAction, manualCommand, manualValue, userCards, userCategories]);

  return (
    <div className="page page-stack winget-page">
      <header className="page-head winget-page-head">
        <div className="winget-heading">
          <img className="winget-page-logo" src={wingetIcon} alt="" />
          <div>
            <p className="page-kicker">Windows 包管理器</p>
            <h2>Winget</h2>
          </div>
        </div>
        <button className="btn" type="button" onClick={() => void onRefresh()} disabled={checking}>
          {checking ? "检测中" : "刷新检测"}
        </button>
      </header>

      <section className="winget-status-panel">
        <div className="winget-status-main">
          <span className={`winget-status-dot${available ? " is-ready" : ""}`} aria-hidden="true" />
          <span className="winget-status-label">状态</span>
          <strong>{statusLabel}</strong>
          {status?.version && <code>{status.version}</code>}
        </div>
        <p>{status?.message ?? "点击刷新检测本机 Winget。"}</p>
      </section>

      <section className="winget-action-panel">
        <div>
          <h3>手动执行</h3>
          <p>复制命令后，在 PowerShell 7 里自行确认和执行。</p>
        </div>
        <div className="winget-action-buttons">
          <button className="btn btn-primary" type="button" onClick={() => void onOpenTerminal()} disabled={!available || checking}>
            打开 PowerShell 7
          </button>
          <button className="btn" type="button" onClick={() => void openUrl("https://learn.microsoft.com/windows/package-manager/winget/")}>
            官方文档
          </button>
        </div>
      </section>

      <section className="winget-card-section" aria-label="自己操作">
        <div className="winget-section-heading">
          <div>
            <p className="winget-section-kicker">自己操作</p>
            <h3>选择软件</h3>
          </div>
          <p>输入软件名称、ID 或完整命令，再选择查找、安装或更新。</p>
        </div>

        <article className="winget-builder-card">
          <div className="winget-builder-controls">
            <label className="winget-name-field">
              <span>软件名称、ID 或 Winget 命令</span>
              <input
                className="winget-card-input"
                value={manualValue}
                onChange={(event) => changeManualValue(event.target.value)}
                placeholder="例如：Steam、Valve.Steam 或 winget install --id Valve.Steam -e"
                aria-label="软件名称、ID 或 Winget 命令"
                spellCheck={false}
              />
            </label>
            <div className="winget-builder-actions" aria-label="针对软件的操作">
              {WINGET_MANUAL_ACTIONS.map((action) => (
                <button
                  className={`winget-builder-action${manualAction === action.id ? " is-active" : ""}`}
                  key={action.id}
                  type="button"
                  aria-pressed={manualAction === action.id}
                  onClick={() => runManualAction(action.id)}
                >
                  {action.label}
                </button>
              ))}
            </div>
          </div>
          <div className="winget-builder-output">
            <code title={manualCommand || "填写后生成命令"}>{manualCommand || "填写后生成命令"}</code>
            <button
              className={`btn btn-sm winget-card-copy${copyState?.id === "manual-builder" && copyState.ok ? " is-copied" : ""}`}
              type="button"
              disabled={!manualCommand}
              onClick={() => void copyCommand("manual-builder", manualCommand)}
            >
              {copyState?.id === "manual-builder" && copyState.ok ? "已复制" : copyState?.id === "manual-builder" && !copyState.ok ? "复制失败" : "复制"}
            </button>
            <button
              className="btn btn-sm"
              type="button"
              disabled={!canSaveManualCommand}
              onClick={saveCurrentCommandAsCard}
            >
              {saveState === "saved" ? "已保存" : saveState === "exists" ? "已存在" : "保存当前命令"}
            </button>
          </div>
        </article>

        {manualAction === "search" && (searching || searchMessage || searchResults.length > 0) && (
          <div className="winget-search-results" aria-live="polite">
            <div className="winget-search-results-head">
              <h4>{searching ? "正在查找" : searchResults.length ? `找到 ${searchResults.length} 个软件` : "搜索结果"}</h4>
              {searchMessage && <span>{searchMessage}</span>}
            </div>
            {searchResults.map((result) => (
              <div className="winget-search-result" key={result.packageId}>
                <div>
                  <strong>{result.name}</strong>
                  <code>{result.packageId}</code>
                  {result.version && <span>{result.version}</span>}
                </div>
                <button className="btn btn-sm btn-primary" type="button" onClick={() => useSearchResult(result)}>
                  使用此 ID
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="winget-card-section winget-user-card-section" aria-label="用户小卡片">
        <div className="winget-section-heading winget-user-card-heading">
          <div>
            <p className="winget-section-kicker">用户小卡片</p>
            <h3>我的命令</h3>
          </div>
          <button className="btn btn-sm winget-add-card-button" type="button" onClick={startAddingCard} title="新建自定义卡片">
            <span aria-hidden="true">+</span>
            新建自定义卡片
          </button>
        </div>

        <div className="winget-category-toolbar">
          <div className="winget-category-list" aria-label="命令分类">
            <button
              className={`winget-category-filter${activeCategoryId === "all" ? " is-active" : ""}`}
              type="button"
              aria-pressed={activeCategoryId === "all"}
              onClick={() => selectCategory("all")}
            >
              全部 <span>{userCards.length}</span>
            </button>
            <button
              className={`winget-category-filter${activeCategoryId === "uncategorized" ? " is-active" : ""}`}
              type="button"
              aria-pressed={activeCategoryId === "uncategorized"}
              onClick={() => selectCategory("uncategorized")}
            >
              未分类 <span>{userCards.filter((card) => !card.categoryId).length}</span>
            </button>
            {userCategories.map((category, categoryIndex) => {
              const isDraggingCategory = draggingCategoryId === category.id;
              const isCategoryDropTarget = categoryDragOverIdx === categoryIndex
                && Boolean(draggingCategoryId)
                && categoryDragOverIdx !== categoryDragFromIdx;
              return (
                <div
                  className={`winget-category-user${isDraggingCategory ? " is-dragging" : ""}${isCategoryDropTarget ? " is-drag-over" : ""}`}
                  key={category.id}
                  data-winget-category-id={category.id}
                  data-winget-category-index={categoryIndex}
                >
                  <button
                    className={`winget-category-filter${activeCategoryId === category.id ? " is-active" : ""}`}
                    type="button"
                    aria-pressed={activeCategoryId === category.id}
                    onPointerDown={(event) => beginUserCategoryDrag(event, category.id, categoryIndex)}
                    onClick={() => selectCategoryFromFilter(category.id)}
                    title="点击切换分类；拖动排序"
                  >
                    {category.name} <span>{userCards.filter((card) => card.categoryId === category.id).length}</span>
                  </button>
                  <button
                    className="winget-category-delete-button"
                    type="button"
                    title="删除分类"
                    aria-label={`删除分类 ${category.name}`}
                    onClick={() => deleteUserCategory(category)}
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
          <div className="winget-category-actions">
            <span className={`winget-selection-count${selectedCards.length > 0 ? " is-active" : ""}`} aria-live="polite">
              已选 {selectedCards.length} 张
            </span>
            <button
              className={`btn btn-sm winget-selection-copy${copyState?.id === "selected-card-set" && copyState.ok ? " is-copied" : ""}`}
              type="button"
              disabled={!combinedCommand}
              onClick={() => void copyCommand("selected-card-set", combinedCommand)}
            >
              {copyState?.id === "selected-card-set" && copyState.ok ? "已复制" : copyState?.id === "selected-card-set" && !copyState.ok ? "复制失败" : "复制组合命令"}
            </button>
            <button className="btn btn-sm btn-primary winget-selection-save" type="button" disabled={!combinedCommand} onClick={saveSelectedCardsAsCard}>
              {combineState === "saved" ? "已生成" : "生成组合卡片"}
            </button>
            <button className="btn btn-sm winget-add-category-button" type="button" onClick={startAddingCategory}>
              <span aria-hidden="true">+</span>
              新建分类
            </button>
          </div>
        </div>

        {draggingCategoryId && draggedUserCategory && (
          <div
            className="winget-category-drag-ghost"
            aria-hidden="true"
            style={{
              width: categoryDragGhostSize.width || undefined,
              minHeight: categoryDragGhostSize.height || undefined,
              transform: `translate3d(${categoryDragPointer.x - categoryDragOffsetRef.current.x}px, ${categoryDragPointer.y - categoryDragOffsetRef.current.y}px, 0) scale(1.04)`,
            }}
          >
            <span className="winget-drag-handle-mark" aria-hidden="true" />
            <strong>{draggedUserCategory.name}</strong>
          </div>
        )}

        {addingCategory && (
          <form className="winget-category-form" onSubmit={addUserCategory}>
            <input
              className="winget-card-input"
              value={draftCategoryName}
              onChange={(event) => setDraftCategoryName(event.target.value)}
              placeholder="分类名称，例如：开发环境"
              aria-label="分类名称"
              maxLength={30}
              autoFocus
            />
            {categoryError && <span className="winget-card-form-error">{categoryError}</span>}
            <div className="winget-card-form-actions">
              <button className="btn btn-sm btn-primary" type="submit">创建分类</button>
              <button className="btn btn-sm" type="button" onClick={cancelAddingCategory}>取消</button>
            </div>
          </form>
        )}

        {addingCard && (
          <form className="winget-card-form" onSubmit={addUserCard}>
            <input
              className="winget-card-input"
              value={draftTitle}
              onChange={(event) => setDraftTitle(event.target.value)}
              placeholder="卡片名称，例如：安装 Chrome"
              aria-label="卡片名称"
              autoFocus
            />
            <select
              className="winget-card-input winget-card-category-select"
              value={draftCategoryId}
              onChange={(event) => setDraftCategoryId(event.target.value)}
              aria-label="卡片分类"
            >
              <option value="">未分类</option>
              {userCategories.map((category) => <option value={category.id} key={category.id}>{category.name}</option>)}
            </select>
            <textarea
              className="winget-card-input winget-card-form-command"
              value={draftCommand}
              onChange={(event) => setDraftCommand(event.target.value)}
              placeholder="命令，例如：winget install Google.Chrome"
              aria-label="卡片命令"
              spellCheck={false}
            />
            <div className="winget-card-form-actions">
              {cardError && <span className="winget-card-form-error">{cardError}</span>}
              <button className="btn btn-sm btn-primary" type="submit">创建卡片</button>
              <button className="btn btn-sm" type="button" onClick={cancelAddingCard}>取消</button>
            </div>
          </form>
        )}

        {visibleUserCards.length > 0 && (
          <div className={`winget-user-card-grid${isCardDragActive ? " is-reordering" : ""}`}>
            {visibleUserCards.map((card, cardIndex) => {
              const cardId = `user-${card.id}`;
              const copyResult = copyState?.id === cardId ? copyState.ok : null;
              const isDraggingCard = draggingCardId === card.id || (cardDragSettling && cardDragFromIdx === cardIndex);
              const shift = isCardDragActive && cardDragFromIdx !== null && cardDragOverIdx !== null && !isDraggingCard
                ? getWingetCardShift(cardIndex, cardDragFromIdx, cardDragOverIdx, cardDragLayoutsRef.current ?? [])
                : { x: 0, y: 0 };
              const cardStyle = shift.x || shift.y ? { transform: `translate3d(${shift.x}px, ${shift.y}px, 0)` } : undefined;
              if (editingCardId === card.id) {
                return (
                  <form className="winget-user-card winget-user-card-editing" key={card.id} onSubmit={updateUserCard}>
                    <input
                      className="winget-card-input"
                      value={editTitle}
                      onChange={(event) => setEditTitle(event.target.value)}
                      aria-label="卡片名称"
                      autoFocus
                    />
                    <select
                      className="winget-card-input winget-card-category-select"
                      value={editCategoryId}
                      onChange={(event) => setEditCategoryId(event.target.value)}
                      aria-label="卡片分类"
                    >
                      <option value="">未分类</option>
                      {userCategories.map((category) => <option value={category.id} key={category.id}>{category.name}</option>)}
                    </select>
                    <textarea
                      className="winget-card-input winget-card-form-command"
                      value={editCommand}
                      onChange={(event) => setEditCommand(event.target.value)}
                      aria-label="卡片命令"
                      spellCheck={false}
                    />
                    <div className="winget-card-form-actions">
                      {editCardError && <span className="winget-card-form-error">{editCardError}</span>}
                      <button className="btn btn-sm btn-primary" type="submit">保存</button>
                      <button className="btn btn-sm" type="button" onClick={cancelEditingCard}>取消</button>
                    </div>
                  </form>
                );
              }
              return (
                <article
                  className={`winget-user-card${isDraggingCard ? " is-dragging" : ""}${cardDragOverIdx === cardIndex && draggingCardId && cardDragOverIdx !== cardDragFromIdx ? " is-drag-over" : ""}`}
                  key={card.id}
                  data-winget-card-id={card.id}
                  data-winget-card-index={cardIndex}
                  style={cardStyle}
                >
                  <div className="winget-user-card-top">
                    <button
                      className="winget-drag-handle"
                      type="button"
                      onPointerDown={(event) => beginUserCardDrag(event, card.id, cardIndex)}
                      title="拖动卡片排序"
                      aria-label={`拖动 ${card.title} 排序`}
                    >
                      <span className="winget-drag-handle-mark" aria-hidden="true" />
                    </button>
                    <h4 title={card.title}>{card.title}</h4>
                  </div>
                  <div className="winget-user-card-footer">
                    <button
                      className={`btn btn-sm winget-card-copy${copyResult === true ? " is-copied" : ""}`}
                      type="button"
                      onClick={() => void copyCommand(cardId, card.command)}
                    >
                      {copyResult === true ? "已复制" : copyResult === false ? "复制失败" : "复制"}
                    </button>
                    <div className="winget-user-card-tools">
                      <label className="winget-select-card" title={`选择 ${card.title}`}>
                        <input
                          type="checkbox"
                          checked={selectedCardIds.includes(card.id)}
                          onChange={() => toggleUserCardSelection(card.id)}
                          aria-label={`选择 ${card.title}`}
                        />
                      </label>
                      <button
                        className="winget-edit-card-button"
                        type="button"
                        onClick={() => startEditingCard(card)}
                        title="编辑卡片"
                        aria-label={`编辑 ${card.title}`}
                      >
                        编辑
                      </button>
                      <button
                        className="winget-delete-card-button"
                        type="button"
                        onClick={() => deleteUserCard(card)}
                        title="删除卡片"
                        aria-label={`删除 ${card.title}`}
                      >
                        ×
                      </button>
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
        )}

        {draggingCardId && draggedUserCard && cardDragFromIdx !== null && (
          <div
            className={`winget-card-drag-ghost${cardDragSettling ? " is-leaving" : ""}`}
            style={{
              width: cardDragGhostSize.width || undefined,
              height: cardDragGhostSize.height || undefined,
              transform: `translate3d(${cardDragPointer.x - cardDragOffsetRef.current.x}px, ${cardDragPointer.y - cardDragOffsetRef.current.y}px, 0) scale(${cardDragSettling ? 0.96 : 1.02})`,
              opacity: cardDragSettling ? 0 : 1,
              filter: cardDragSettling ? "blur(2px)" : undefined,
              transition: cardDragSettling
                ? "opacity 0.28s cubic-bezier(0.22, 1, 0.36, 1), transform 0.28s cubic-bezier(0.22, 1, 0.36, 1), filter 0.28s cubic-bezier(0.22, 1, 0.36, 1)"
                : undefined,
            }}
          >
            <div className="winget-card-drag-ghost-head">
              <span className="winget-drag-handle-mark" aria-hidden="true" />
              <strong title={draggedUserCard.title}>{draggedUserCard.title}</strong>
            </div>
          </div>
        )}
      </section>

    </div>
  );
});

const DEFAULT_STEP_DRAFT: StepDraft = {
  name: "",
  action: "click",
  windowTitle: "WeGame",
  matchType: "text",
  matchValue: "",
  colorTolerance: 24,
  click: true,
  offsetX: 0,
  offsetY: 0,
  inputMode: "installBase",
  inputText: "",
};

function stepToDraft(step: AutomationStep): StepDraft {
  return {
    name: step.name,
    action: step.action ?? "click",
    windowTitle: step.windowTitle,
    matchType: step.matchType,
    matchValue: step.matchValue,
    colorTolerance: step.colorTolerance,
    click: step.click,
    offsetX: step.offsetX ?? 0,
    offsetY: step.offsetY ?? 0,
    inputMode: step.inputMode ?? "installBase",
    inputText: step.inputText ?? "",
  };
}

function normalizeStep(step: AutomationStep & { waitMode?: string }): AutomationStep {
  return {
    ...step,
    enabled: step.enabled ?? true,
    timeAfterStep: step.timeAfterStep ?? step.waitMode === "manual",
    delayMs: step.delayMs ?? 0,
    delayUnit: step.delayUnit === "s" ? "s" : "ms",
  };
}

function delayUnitOf(step: AutomationStep): "ms" | "s" {
  return step.delayUnit === "s" ? "s" : "ms";
}

function delayDisplayValue(step: AutomationStep): number {
  if (delayUnitOf(step) === "s") return Number(((step.delayMs ?? 0) / 1000).toFixed(2));
  return step.delayMs ?? 0;
}

function delayValueToMs(value: number, unit: "ms" | "s"): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.round(unit === "s" ? value * 1000 : value));
}

function formatDelay(ms: number, unit: "ms" | "s" = "ms"): string {
  if (unit === "s") return `${Number((ms / 1000).toFixed(2))}s`;
  return `${ms}ms`;
}

function draftFromStep(step: Partial<AutomationStep> & { id: string }): AutomationStep {
  return {
    id: step.id,
    name: step.name ?? "",
    action: step.action ?? "click",
    windowTitle: step.windowTitle ?? "WeGame",
    matchType: step.matchType ?? "text",
    matchValue: step.matchValue ?? "",
    colorTolerance: step.colorTolerance ?? 24,
    click: step.click ?? true,
    enabled: step.enabled ?? true,
    delayMs: step.delayMs ?? 1000,
    delayUnit: step.delayUnit === "s" ? "s" : "ms",
    timeAfterStep: step.timeAfterStep ?? false,
    lastMeasuredMs: step.lastMeasuredMs ?? 0,
    offsetX: step.offsetX ?? 0,
    offsetY: step.offsetY ?? 0,
    inputMode: step.inputMode ?? "installBase",
    inputText: step.inputText ?? "",
  };
}

function stepTypeLabel(step: AutomationStep): string {
  if (step.action === "closeWindow") return "关闭窗口";
  if (step.action === "inputText") return "输入路径";
  if (step.matchType === "point") return "准星";
  if (step.matchType === "color") return "颜色";
  return "文字";
}

function getStepRowShift(idx: number, from: number, over: number, rowH: number): number {
  if (from === over) return 0;
  if (from < over) {
    if (idx > from && idx <= over) return -rowH;
  } else if (idx >= over && idx < from) {
    return rowH;
  }
  return 0;
}

interface StepRowLayoutSlot {
  top: number;
  bottom: number;
}

function captureStepRowLayouts(tbody: HTMLElement): StepRowLayoutSlot[] {
  return Array.from(tbody.querySelectorAll<HTMLTableRowElement>("tr[data-step-idx]")).map((row) => {
    const rect = row.getBoundingClientRect();
    return { top: rect.top, bottom: rect.bottom };
  });
}

function resolveStepDropIndex(clientY: number, layouts: StepRowLayoutSlot[]): number {
  if (layouts.length === 0) return 0;
  for (let i = 0; i < layouts.length; i++) {
    const { top, bottom } = layouts[i];
    if (clientY >= top && clientY < bottom) return i;
  }
  if (clientY < layouts[0].top) return 0;
  return layouts.length - 1;
}

const STEP_DRAG_SETTLE_MS = 300;

type LibrarySourceFilter = "all" | "github" | "official" | "store";

const APP_THEME: Record<string, string> = {
  stranslate: "theme-violet",
  quickclipboard: "theme-mint",
  leagueakari: "theme-amber",
  wegame: "theme-wegame",
};

function softwareSourceKind(sw: SoftwareInfo): "github" | "official" | "store" {
  if (sw.source_kind === "github" || sw.source_kind === "official" || sw.source_kind === "store") return sw.source_kind;
  if (sw.install_kind === "store") return "store";
  return sw.install_kind === "installer" ? "official" : "github";
}

function softwareSourceLabel(sw: SoftwareInfo): string {
  const source = softwareSourceKind(sw);
  if (source === "github") return "GitHub";
  if (source === "store") return "微软商店";
  return "官网";
}

function softwareKindLabel(sw: SoftwareInfo): string {
  if (sw.install_kind === "store") return "商店应用";
  if (sw.install_kind === "download") return "仅下载";
  return sw.install_kind === "installer" ? "安装包" : "便携版";
}

function versionDisplay(sw: SoftwareInfo): string {
  const version = sw.latest_version.trim();
  if (!isDownloadOnly(sw)) return version;
  if (!version || version === "自定义下载") return "固定链接";
  if (version === "最新版本") return "自动解析";
  return version;
}

function isDownloadOnly(sw: SoftwareInfo) {
  return sw.install_kind === "download";
}

function canRunSilentInstall(sw: SoftwareInfo) {
  return !!sw.silent_install_args.trim() && !!sw.portable && /\.exe$/i.test(sw.portable.name);
}

function isMicrosoftStoreApp(sw: SoftwareInfo) {
  return sw.install_kind === "store" || softwareSourceKind(sw) === "store";
}

function microsoftStoreProductId(sw: SoftwareInfo) {
  const match = sw.release_url.match(/apps\.microsoft\.com\/detail\/([a-z0-9]{12})/i);
  return match?.[1]?.toUpperCase() ?? "";
}

function isUserSource(sw: SoftwareInfo) {
  return !BUILT_IN_SOFTWARE_IDS.has(sw.id);
}

function iconSrc(path: string) {
  return path && /^(?:data:|https?:)/i.test(path) ? path : path ? convertFileSrc(path) : "";
}

function softwareIconSrc(sw: Pick<SoftwareInfo, "id" | "icon_path">) {
  return BUILT_IN_ICON_SOURCES[sw.id] ?? iconSrc(sw.icon_path);
}

/*  helpers  */

function formatSize(bytes: number): string {
  if (!bytes) return "";
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

function formatDate(iso: string): string {
  if (!iso) return "";
  try {
    const d = new Date(iso);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  } catch {
    return iso;
  }
}

function formatDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function formatDurationShort(ms: number): string {
  const safe = Math.max(0, ms);
  if (safe < 10_000) return `${(safe / 1000).toFixed(1)}s`;
  return formatDuration(safe);
}

function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function splitPath(path: string): string[] {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).filter(Boolean);
}

/*  icon components  */

function IconFolder({ className = "" }: { className?: string }) {
  return (
    <svg className={`dir-icon ${className}`} viewBox="0 0 16 16" aria-hidden="true">
      <path fill="currentColor" d="M1.5 3.5A1.5 1.5 0 0 1 3 2h3.17a1.5 1.5 0 0 1 1.06.44L8.12 3.5H13a1.5 1.5 0 0 1 1.5 1.5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V3.5z" />
    </svg>
  );
}

function IconFile({ className = "" }: { className?: string }) {
  return (
    <svg className={`dir-icon ${className}`} viewBox="0 0 16 16" aria-hidden="true">
      <path fill="currentColor" d="M4 1.5A1.5 1.5 0 0 1 5.5 0h3.88a1.5 1.5 0 0 1 1.06.44l2.12 2.12A1.5 1.5 0 0 1 13 3.62V12.5A1.5 1.5 0 0 1 11.5 14h-7A1.5 1.5 0 0 1 3 12.5v-11z" />
    </svg>
  );
}

function IconDrive({ className = "" }: { className?: string }) {
  return (
    <svg className={`dir-icon ${className}`} viewBox="0 0 16 16" aria-hidden="true">
      <path fill="currentColor" d="M2 4.5A2.5 2.5 0 0 1 4.5 2h7A2.5 2.5 0 0 1 14 4.5v7A2.5 2.5 0 0 1 11.5 14h-7A2.5 2.5 0 0 1 2 11.5v-7zm1 0v7a.5.5 0 0 0 .5.5h7a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.5-.5h-7a.5.5 0 0 0-.5.5z" />
      <path fill="currentColor" d="M5 11.5h6v1H5v-1z" opacity="0.55" />
    </svg>
  );
}

function IconLink({ className = "" }: { className?: string }) {
  return (
    <svg className={`dir-icon ${className}`} viewBox="0 0 16 16" aria-hidden="true">
      <path fill="currentColor" d="M6.5 4.5a3 3 0 0 1 4.24 0l1.5 1.5a3 3 0 0 1-4.24 4.24l-.75-.75a.75.75 0 1 1 1.06-1.06l.75.75a1.5 1.5 0 1 0 2.12-2.12l-1.5-1.5a1.5 1.5 0 0 0-2.12 0 .75.75 0 1 1-1.06-1.06z" />
      <path fill="currentColor" d="M9.5 11.5a3 3 0 0 1-4.24 0l-1.5-1.5a3 3 0 0 1 4.24-4.24l.75.75a.75.75 0 0 1-1.06 1.06l-.75-.75a1.5 1.5 0 1 0-2.12 2.12l1.5 1.5a1.5 1.5 0 0 0 2.12 0 .75.75 0 0 1 1.06 1.06z" />
    </svg>
  );
}

/*  DirExplorer (used in Settings page)  */

function DirExplorer({
  base,
  software,
  selectedId,
  appPaths,
  installed,
  packageCache,
}: {
  base: string;
  software: SoftwareInfo[];
  selectedId: string | null;
  appPaths: Record<string, AppInstallPaths>;
  installed: Record<string, boolean>;
  packageCache: Record<string, PackageCacheInfo>;
}) {
  const segments = splitPath(base);
  const rootLabel = segments[segments.length - 1] || base || "apps";
  const items = software.filter((sw) => sw.portable);
  const cachedItems = items.filter((sw) => packageCache[sw.id]?.cached);
  const selectedPaths = selectedId ? appPaths[selectedId] : null;
  const selectedSw = selectedId ? software.find((s) => s.id === selectedId) : null;

  return (
    <div className="dir-explorer">
      <div className="dir-breadcrumb" title={base}>
        <IconDrive className="dir-icon-drive" />
        <div className="dir-crumb-track">
          {segments.map((seg, i) => (
            <span key={`${seg}-${i}`} className="dir-crumb">
              {i > 0 && <span className="dir-crumb-sep">/</span>}
              <span className={`dir-crumb-part${i === segments.length - 1 ? " is-current" : ""}`}>{seg}</span>
            </span>
          ))}
        </div>
      </div>

      <div className="dir-tree-scroll">
        <div className="dir-tree">
          <div className="dir-node dir-node-root">
            <IconFolder className="dir-icon-folder-root" />
            <span className="dir-node-label">{rootLabel}</span>
            <span className="dir-node-meta">{items.length} </span>
          </div>

          {items.map((sw) => {
            const theme = APP_THEME[sw.id] || "theme-mint";
            const active = selectedId === sw.id;
            const paths = appPaths[sw.id];
            return (
              <div key={sw.id} className={`dir-branch ${theme}${active ? " is-active" : ""}${installed[sw.id] ? " is-installed" : ""}`}>
                <div className="dir-node dir-node-folder">
                  <span className="dir-guide" aria-hidden="true" />
                  <IconFolder className="dir-icon-folder" />
                  <span className="dir-node-label">{sw.display_name}</span>
                  {installed[sw.id] && <span className="dir-pill">已安装</span>}
                </div>
                {active && paths && (
                  <div className="dir-detail">
                    <div className="dir-detail-title">路径详情</div>
                    <div className="dir-detail-row">
                      <span className="dir-detail-key">安装目录</span><code className="dir-detail-val">{paths.install_dir}</code>
                    </div>
                    <div className="dir-detail-row">
                      <span className="dir-detail-key">缓存安装包</span><code className="dir-detail-val">{paths.package_file}</code>
                    </div>
                    <div className="dir-detail-row">
                      <span className="dir-detail-key">
                        <IconLink className="dir-icon-inline" />
                        
                      </span>
                      <code className="dir-detail-val">{paths.shortcut}</code>
                    </div>
                  </div>
                )}
              </div>
            );
          })}

          <div className="dir-section-divider" />

          <div className="dir-node dir-node-root">
            <IconFolder className="dir-icon-folder-root" />
            <span className="dir-node-label">packages</span>
            <span className="dir-node-meta">{cachedItems.length}/{items.length} </span>
          </div>

          {items.map((sw) => {
            const theme = APP_THEME[sw.id] || "theme-mint";
            const cached = packageCache[sw.id]?.cached;
            return (
              <div key={`${sw.id}-package`} className={`dir-branch ${theme}`}>
                <div className="dir-node dir-node-folder">
                  <span className="dir-guide" aria-hidden="true" />
                  <IconFolder className="dir-icon-folder" />
                  <span className="dir-node-label">{sw.display_name}</span>
                  <span className="dir-node-meta">{sw.latest_version}</span>
                  {cached && <span className="dir-pill dir-pill-cache">已缓存</span>}
                </div>
                {sw.portable && (
                  <div className="dir-node dir-node-file">
                    <span className="dir-guide dir-guide-deep" aria-hidden="true" />
                    <IconFile className="dir-icon-file" />
                    <span className="dir-node-label dir-file-name" title={appPaths[sw.id]?.package_file ?? sw.portable.name}>
                      {sw.portable.name}
                    </span>
                  </div>
                )}
              </div>
            );
          })}

          {items.length === 0 && <div className="dir-empty">暂无软件目录</div>}
        </div>
      </div>

      {selectedSw && !selectedPaths && (
        <div className="dir-footnote">选中「{selectedSw.display_name}」查看路径</div>
      )}
    </div>
  );
}

/*  WindowControls  */

function WindowControls() {
  const win = getCurrentWindow();

  async function closeWindow() {
    try {
      await win.destroy();
    } catch {
      try {
        await win.close();
      } catch {
        await invoke("exit_app");
      }
    }
  }

  return (
    <div className="window-controls">
      <button className="win-btn" type="button" aria-label="最小化" onPointerDown={(e) => e.stopPropagation()} onClick={() => void win.minimize()}>
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6.5h8" stroke="currentColor" strokeWidth="1.2" /></svg>
      </button>
      <button className="win-btn" type="button" aria-label="最大化" onPointerDown={(e) => e.stopPropagation()} onClick={() => void win.toggleMaximize()}>
        <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1.2" /></svg>
      </button>
      <button className="win-btn win-btn-close" type="button" aria-label="最小化" onPointerDown={(e) => e.stopPropagation()} onClick={() => void closeWindow()}>
        <svg viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 3l6 6M9 3L3 9" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}

/* ?
   PAGE: ?
   ?*/

function AutomationPage({ software }: { software: SoftwareInfo[] }) {
  const [steps, setSteps] = useState<AutomationStep[]>([]);
  const [templates, setTemplates] = useState<AutomationTemplate[]>([]);
  const [activeTemplateId, setActiveTemplateId] = useState("");
  const [templateName, setTemplateName] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<StepDraft>(DEFAULT_STEP_DRAFT);
  const [isElevated, setIsElevated] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [running, setRunning] = useState(false);
  const [feedback, setFeedback] = useState("");

  async function openWeGameInstaller() {
    const sw = software.find((s) => s.id === "wegame");
    if (!sw || !sw.portable) return alert("未找到 WeGame 安装包信息，请先加载软件库。");
    try {
      setBusy(true);
      const launch = await invoke<WeGameInstallResult>("launch_wegame_installer_cmd", {
        id: sw.id,
        version: sw.latest_version,
        fileName: sw.portable.name,
      });
      if (!launch.success) throw new Error(launch.message || "");
    } catch (e) {
      alert(`打开安装包失败：${e}`);
    } finally {
      setBusy(false);
    }
  }
  const [previewImage, setPreviewImage] = useState("");
  const [previewResult, setPreviewResult] = useState<VisualTargetResult | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [totalStartedAt, setTotalStartedAt] = useState<number | null>(null);
  const [totalElapsedMs, setTotalElapsedMs] = useState(0);
  const [stepStartedAt, setStepStartedAt] = useState<number | null>(null);
  const [stepElapsedMs, setStepElapsedMs] = useState(0);
  const [stepTimerName, setStepTimerName] = useState("");
  const [runningStepId, setRunningStepId] = useState<string | null>(null);
  const [awaitingPhaseStep, setAwaitingPhaseStep] = useState<AutomationStep | null>(null);
  const [dragStepId, setDragStepId] = useState<string | null>(null);
  const [dragOverIdx, setDragOverIdx] = useState<number | null>(null);
  const [dragFromIdx, setDragFromIdx] = useState<number | null>(null);
  const [dragPointer, setDragPointer] = useState({ x: 0, y: 0 });
  const [rowHeight, setRowHeight] = useState(44);
  const [ghostWidth, setGhostWidth] = useState(0);
  const [dragSettling, setDragSettling] = useState(false);
  const dragFromIdxRef = useRef<number | null>(null);
  const dragOverIdxRef = useRef<number | null>(null);
  const dragOffsetRef = useRef({ x: 24, y: 22 });
  const dragRafRef = useRef<number | null>(null);
  const settleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dragRowLayoutsRef = useRef<StepRowLayoutSlot[] | null>(null);
  const stepStartedAtRef = useRef<number | null>(null);
  const phaseCompleteRef = useRef<(() => void) | null>(null);

  const selected = steps.find((s) => s.id === selectedId) ?? null;
  const activeTemplate = templates.find((template) => template.id === activeTemplateId) ?? null;
  const runnableStepCount = steps.filter((s) => s.enabled !== false).length;
  const timerRunning = totalStartedAt !== null || stepStartedAt !== null;
  const phaseTimerActive = awaitingPhaseStep !== null && stepStartedAt !== null;
  const draggedStep = dragStepId ? steps.find((s) => s.id === dragStepId) ?? null : null;
  const isDragActive = dragStepId !== null || dragSettling;

  const loadSteps = useCallback(async () => {
    try {
      const [list, templateList, activeId] = await Promise.all([
        invoke<AutomationStep[]>("get_automation_steps_cmd"),
        invoke<AutomationTemplate[]>("get_automation_templates_cmd"),
        invoke<string>("get_active_automation_template_cmd"),
      ]);
      const normalized = list.map(normalizeStep);
      setSteps(normalized);
      setTemplates(templateList);
      setActiveTemplateId(activeId);
      const active = templateList.find((template) => template.id === activeId) ?? templateList[0] ?? null;
      setTemplateName(active?.name ?? "");
      setSelectedId((prev) => {
        if (normalized.length === 0) return null;
        if (prev && normalized.some((s) => s.id === prev)) return prev;
        return normalized[0].id;
      });
    } catch {
      setSteps([]);
      setTemplates([]);
      setActiveTemplateId("");
      setTemplateName("");
      setSelectedId(null);
    }
  }, []);

  useEffect(() => {
    void loadSteps();
  }, [loadSteps]);

  useEffect(() => {
    invoke<boolean>("is_elevated_cmd")
      .then(setIsElevated)
      .catch(() => setIsElevated(null));
  }, []);

  useEffect(() => {
    const step = steps.find((s) => s.id === selectedId);
    if (step) setDraft(stepToDraft(step));
    else setDraft(DEFAULT_STEP_DRAFT);
  }, [selectedId, steps]);

  useEffect(() => {
    if (totalStartedAt === null && stepStartedAt === null) return;
    const tick = () => {
      if (totalStartedAt !== null) setTotalElapsedMs(Date.now() - totalStartedAt);
      if (stepStartedAt !== null) setStepElapsedMs(Date.now() - stepStartedAt);
    };
    tick();
    const id = window.setInterval(tick, 200);
    return () => window.clearInterval(id);
  }, [totalStartedAt, stepStartedAt]);

  function beginTotalTimer() {
    const now = Date.now();
    setTotalStartedAt(now);
    setTotalElapsedMs(0);
  }

  function beginStepTimer(label: string) {
    const now = Date.now();
    stepStartedAtRef.current = now;
    setStepTimerName(label);
    setStepStartedAt(now);
    setStepElapsedMs(0);
  }

  function endStepTimer() {
    if (stepStartedAtRef.current !== null) {
      setStepElapsedMs(Date.now() - stepStartedAtRef.current);
    }
    stepStartedAtRef.current = null;
    setStepStartedAt(null);
    setStepTimerName("");
  }

  function stopExecutionTimer() {
    if (totalStartedAt !== null) setTotalElapsedMs(Date.now() - totalStartedAt);
    if (stepStartedAtRef.current !== null) setStepElapsedMs(Date.now() - stepStartedAtRef.current);
    setTotalStartedAt(null);
    stepStartedAtRef.current = null;
    setStepStartedAt(null);
    setStepTimerName("");
    setRunningStepId(null);
    setAwaitingPhaseStep(null);
    if (phaseCompleteRef.current) {
      phaseCompleteRef.current();
      phaseCompleteRef.current = null;
    }
  }

  function resetExecutionTimer() {
    setTotalStartedAt(null);
    stepStartedAtRef.current = null;
    setStepStartedAt(null);
    setTotalElapsedMs(0);
    setStepElapsedMs(0);
    setStepTimerName("");
    setRunningStepId(null);
    setAwaitingPhaseStep(null);
  }

  async function persistSteps(next: AutomationStep[]) {
    setSteps(next);
    setTemplates((prev) =>
      prev.map((template) =>
        template.id === activeTemplateId ? { ...template, steps: next, updatedAt: Date.now() } : template
      )
    );
    try {
      await invoke("save_automation_steps_cmd", { steps: next });
    } catch (e) {
      setFeedback(`: ${e}`);
    }
  }

  async function saveCurrentAsTemplate() {
    const name = templateName.trim();
    if (!name) {
      setFeedback("");
      return;
    }
    setBusy(true);
    try {
      const template = await invoke<AutomationTemplate>("save_automation_template_cmd", { name, steps });
      const nextSteps = template.steps.map(normalizeStep);
      setTemplates((prev) => [...prev.filter((item) => item.id !== template.id), template]);
      setActiveTemplateId(template.id);
      setSteps(nextSteps);
      setSelectedId(nextSteps[0]?.id ?? null);
      setDraft(nextSteps[0] ? stepToDraft(nextSteps[0]) : DEFAULT_STEP_DRAFT);
      setFeedback(`已保存模板「${template.name}」`);
      setPreviewImage("");
      setPreviewResult(null);
    } catch (e) {
      setFeedback(`操作失败: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function switchTemplate(id: string) {
    if (!id || id === activeTemplateId || running) return;
    setBusy(true);
    try {
      const next = (await invoke<AutomationStep[]>("set_active_automation_template_cmd", { templateId: id })).map(normalizeStep);
      const template = templates.find((item) => item.id === id) ?? null;
      setActiveTemplateId(id);
      setTemplateName(template?.name ?? "");
      setSteps(next);
      setSelectedId(next[0]?.id ?? null);
      setDraft(next[0] ? stepToDraft(next[0]) : DEFAULT_STEP_DRAFT);
      setLog([]);
      setFeedback(template ? `已切换到模板「${template.name}」` : "已切换模板");
      setPreviewImage("");
      setPreviewResult(null);
    } catch (e) {
      setFeedback(`操作失败: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function deleteActiveTemplate() {
    if (!activeTemplateId || running || templates.length <= 1) return;
    const name = activeTemplate?.name ?? "";
    if (!window.confirm(`确定删除模板「${name}」？`)) return;
    setBusy(true);
    try {
      const nextTemplates = await invoke<AutomationTemplate[]>("delete_automation_template_cmd", { templateId: activeTemplateId });
      setTemplates(nextTemplates);
      const activeId = await invoke<string>("get_active_automation_template_cmd");
      const next = (await invoke<AutomationStep[]>("get_automation_steps_cmd")).map(normalizeStep);
      const active = nextTemplates.find((template) => template.id === activeId) ?? nextTemplates[0] ?? null;
      setActiveTemplateId(activeId);
      setTemplateName(active?.name ?? "");
      setSteps(next);
      setSelectedId(next[0]?.id ?? null);
      setDraft(next[0] ? stepToDraft(next[0]) : DEFAULT_STEP_DRAFT);
      setFeedback(`已删除模板「${name}」`);
      setPreviewImage("");
      setPreviewResult(null);
    } catch (e) {
      setFeedback(`操作失败: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  function selectStep(step: AutomationStep) {
    setSelectedId(step.id);
    setDraft(stepToDraft(step));
    setFeedback("");
    setPreviewImage("");
    setPreviewResult(null);
  }

  function addStep() {
    const step = draftFromStep({
      id: `step-${Date.now()}`,
      name: `新步骤 ${steps.length + 1}`,
    });
    const next = [...steps, step];
    void persistSteps(next);
    selectStep(step);
    setFeedback(`已添加「${step.name}」`);
  }

  async function restartAsAdmin() {
    setFeedback("");
    try {
      await invoke("restart_as_admin_cmd");
    } catch (e) {
      setFeedback(`操作失败: ${e}`);
    }
  }

  function removeStep(id: string) {
    const next = steps.filter((s) => s.id !== id);
    void persistSteps(next);
    if (selectedId === id) {
      const fallback = next[0] ?? null;
      setSelectedId(fallback?.id ?? null);
      setDraft(fallback ? stepToDraft(fallback) : DEFAULT_STEP_DRAFT);
    }
  }

  function reorderStep(fromIdx: number, toIdx: number) {
    if (fromIdx === toIdx || fromIdx < 0 || toIdx < 0 || fromIdx >= steps.length || toIdx >= steps.length) return;
    const next = [...steps];
    const [moved] = next.splice(fromIdx, 1);
    next.splice(toIdx, 0, moved);
    void persistSteps(next);
  }

  function clearDragState() {
    if (settleTimerRef.current !== null) {
      window.clearTimeout(settleTimerRef.current);
      settleTimerRef.current = null;
    }
    dragFromIdxRef.current = null;
    dragOverIdxRef.current = null;
    dragRowLayoutsRef.current = null;
    setDragStepId(null);
    setDragOverIdx(null);
    setDragFromIdx(null);
    setDragSettling(false);
  }

  function beginStepDrag(e: PointerEvent, idx: number) {
    if (running || e.button !== 0) return;
    if (settleTimerRef.current !== null) {
      clearDragState();
    }
    e.preventDefault();
    e.stopPropagation();
    const rowEl = (e.currentTarget as HTMLElement).closest<HTMLTableRowElement>("tr[data-step-idx]");
    if (rowEl) {
      const rect = rowEl.getBoundingClientRect();
      dragOffsetRef.current = { x: e.clientX - rect.left, y: e.clientY - rect.top };
      setRowHeight(rect.height);
      const table = rowEl.closest("table");
      if (table) setGhostWidth(table.getBoundingClientRect().width);
      const tbody = rowEl.closest("tbody");
      if (tbody) dragRowLayoutsRef.current = captureStepRowLayouts(tbody);
    }
    dragFromIdxRef.current = idx;
    dragOverIdxRef.current = idx;
    setDragFromIdx(idx);
    setDragStepId(steps[idx].id);
    setDragOverIdx(idx);
    setDragPointer({ x: e.clientX, y: e.clientY });
  }

  useEffect(() => {
    if (!dragStepId) return;

    const onMove = (e: globalThis.PointerEvent) => {
      if (dragFromIdxRef.current === null) return;
      const x = e.clientX;
      const y = e.clientY;
      if (dragRafRef.current !== null) cancelAnimationFrame(dragRafRef.current);
      dragRafRef.current = requestAnimationFrame(() => {
        setDragPointer({ x, y });
        const layouts = dragRowLayoutsRef.current;
        if (layouts) {
          const idx = resolveStepDropIndex(y, layouts);
          if (idx !== dragOverIdxRef.current) {
            dragOverIdxRef.current = idx;
            setDragOverIdx(idx);
          }
        }
        dragRafRef.current = null;
      });
    };

    const finish = () => {
      if (dragRafRef.current !== null) {
        cancelAnimationFrame(dragRafRef.current);
        dragRafRef.current = null;
      }
      const from = dragFromIdxRef.current;
      const to = dragOverIdxRef.current;

      if (from !== null && to !== null && from !== to) {
        dragFromIdxRef.current = null;
        dragOverIdxRef.current = null;
        setDragStepId(null);
        setDragOverIdx(null);
        setDragFromIdx(null);
        setDragSettling(false);
        reorderStep(from, to);
        return;
      }

      if (from !== null) {
        document.body.classList.remove("is-step-dragging");
        document.body.style.userSelect = "";
        dragOverIdxRef.current = from;
        setDragOverIdx(from);
        setDragSettling(true);
        settleTimerRef.current = window.setTimeout(() => {
          clearDragState();
        }, STEP_DRAG_SETTLE_MS);
        return;
      }

      clearDragState();
    };

    document.body.style.userSelect = "none";
    document.body.classList.add("is-step-dragging");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    return () => {
      document.body.style.userSelect = "";
      document.body.classList.remove("is-step-dragging");
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
  }, [dragStepId]);

  useEffect(() => () => {
    if (settleTimerRef.current !== null) {
      window.clearTimeout(settleTimerRef.current);
    }
  }, []);

  function setStepEnabled(id: string, enabled: boolean) {
    const next = steps.map((s) => (s.id === id ? { ...s, enabled } : s));
    void persistSteps(next);
  }

  function setStepDelayValue(id: string, value: number) {
    const next = steps.map((s) => {
      if (s.id !== id) return s;
      const unit = delayUnitOf(s);
      return { ...s, delayMs: delayValueToMs(value, unit) };
    });
    void persistSteps(next);
  }

  function setStepDelayUnit(id: string, delayUnit: "ms" | "s") {
    const next = steps.map((s) => (s.id === id ? { ...s, delayUnit } : s));
    void persistSteps(next);
  }

  function setStepTimeAfter(id: string, timeAfterStep: boolean) {
    const next = steps.map((s) => (s.id === id ? { ...s, timeAfterStep } : s));
    void persistSteps(next);
  }

  function recordStepMeasured(stepId: string, ms: number) {
    const measured = Math.max(0, Math.round(ms));
    const next = steps.map((s) => (s.id === stepId ? { ...s, lastMeasuredMs: measured } : s));
    void persistSteps(next);
  }

  function finishPhaseTimer(step: AutomationStep | null = awaitingPhaseStep) {
    if (step && stepStartedAtRef.current !== null) {
      const elapsed = Date.now() - stepStartedAtRef.current;
      setStepElapsedMs(elapsed);
      recordStepMeasured(step.id, elapsed);
      setLog((prev) => [...prev, `计时完成「${step.name}」· ${formatDurationShort(elapsed)}`]);
      setFeedback(`步骤「${step.name}」完成 · ${formatDurationShort(elapsed)}`);
    }
    endStepTimer();
    setAwaitingPhaseStep(null);
    setRunningStepId(null);
  }

  function completePhaseTimer() {
    if (phaseCompleteRef.current) {
      const complete = phaseCompleteRef.current;
      phaseCompleteRef.current = null;
      complete();
      return;
    }
    finishPhaseTimer();
  }

  async function saveDraftToStep() {
    if (!selectedId) return;
    const needsMatch = draft.action !== "closeWindow" && draft.matchType !== "point";
    const needsPoint = draft.action !== "closeWindow" && draft.matchType === "point";
    if (!draft.name.trim() || (needsMatch && !draft.matchValue.trim()) || (needsPoint && !draft.matchValue.trim())) {
      setFeedback(needsPoint ? "请先用准星选择坐标" : "请填写步骤名称和匹配内容");
      return;
    }
    const next = steps.map((s) =>
      s.id === selectedId
        ? {
            ...s,
            name: draft.name.trim(),
            action: draft.action,
            windowTitle: draft.windowTitle.trim(),
            matchType: draft.matchType,
            matchValue: draft.matchValue.trim(),
            colorTolerance: draft.colorTolerance,
            click: draft.action === "closeWindow" ? false : draft.click,
            offsetX: draft.matchType === "point" ? 0 : draft.offsetX,
            offsetY: draft.matchType === "point" ? 0 : draft.offsetY,
            inputMode: draft.inputMode,
            inputText: draft.inputText.trim(),
          }
        : s
    );
    await persistSteps(next);
    setFeedback(`已保存「${draft.name.trim()}」`);
  }

  async function runLocator(dryRun: boolean) {
    if (draft.action === "closeWindow") {
      await closeTargetWindow(draft.windowTitle.trim() || "WeGame");
      return;
    }
    if (draft.matchType !== "point" && !draft.matchValue.trim()) {
      setFeedback("");
      return;
    }
    if (draft.matchType === "point" && !dryRun && !draft.matchValue.trim()) {
      setFeedback("");
      return;
    }
    setBusy(true);
    setFeedback(dryRun ? "" : "");
    if (!dryRun) beginStepTimer(draft.name.trim() || "");
    if (dryRun) {
      setPreviewImage("");
      setPreviewResult(null);
    }
    try {
      const r = await invoke<VisualTargetResult>("run_visual_target_cmd", {
        action: draft.action,
        matchType: draft.matchType,
        matchValue: draft.matchValue.trim(),
        windowTitle: draft.windowTitle.trim(),
        colorTolerance: draft.colorTolerance,
        click: dryRun ? false : draft.click || draft.action === "inputText",
        dryRun,
        offsetX: draft.offsetX,
        offsetY: draft.offsetY,
        inputMode: draft.inputMode,
        inputText: draft.inputText.trim(),
      });
      const pos = r.screen_x != null && r.screen_y != null ? ` (${r.screen_x}, ${r.screen_y})` : "";
      setFeedback(`${r.message}${pos}`);
      if (dryRun && r.preview_image) {
        setPreviewImage(r.preview_image);
        setPreviewResult(r);
      }
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickColorFromMouse() {
    setBusy(true);
    setFeedback("1.2 ");
    try {
      const color = await invoke<PickedScreenColor>("pick_screen_color_cmd", { delayMs: 1200 });
      setDraft((d) => ({
        ...d,
        matchType: "color",
        matchValue: color.hex,
      }));
      setFeedback(`已拾取 ${color.hex} (${color.screen_x}, ${color.screen_y})`);
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setBusy(false);
    }
  }

  function manualPointValueFromResult(result = previewResult) {
    if (
      !result ||
      result.screen_x == null ||
      result.screen_y == null ||
      result.window_left == null ||
      result.window_top == null
    ) {
      return "";
    }
    return `window:${Math.round(result.screen_x - result.window_left)},${Math.round(result.screen_y - result.window_top)}`;
  }

  function choosePointMode() {
    const pointValue = manualPointValueFromResult();
    setDraft((d) => ({
      ...d,
      matchType: "point",
      matchValue: pointValue || d.matchValue,
      offsetX: 0,
      offsetY: 0,
    }));
    setFeedback(pointValue ? `已选择准星坐标 ${pointValue}` : "请先预览并拖动准星选择坐标");
  }

  function useCurrentCrosshair() {
    const pointValue = manualPointValueFromResult();
    if (!pointValue) {
      setFeedback("");
      return;
    }
    setDraft((d) => ({ ...d, matchType: "point", matchValue: pointValue, offsetX: 0, offsetY: 0 }));
    setFeedback(`已选择准星坐标 ${pointValue}`);
  }

  async function closeTargetWindow(title: string) {
    setBusy(true);
    setFeedback(`正在关闭「${title || "WeGame"}」…`);
    try {
      const r = await invoke<VisualTargetResult>("close_target_window_cmd", { windowTitle: title || "WeGame" });
      setFeedback(`${r.success ? "✓" : "✗"} ${r.message}`);
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setBusy(false);
    }
  }

  function previewMarkerStyle() {
    if (
      !previewResult ||
      previewResult.screen_x == null ||
      previewResult.screen_y == null ||
      previewResult.window_left == null ||
      previewResult.window_top == null ||
      !previewResult.window_width ||
      !previewResult.window_height
    ) {
      return null;
    }
    return {
      left: `${clampNumber(((previewResult.screen_x - previewResult.window_left) / previewResult.window_width) * 100, 0, 100)}%`,
      top: `${clampNumber(((previewResult.screen_y - previewResult.window_top) / previewResult.window_height) * 100, 0, 100)}%`,
    };
  }

  function updatePreviewPoint(e: PointerEvent<HTMLDivElement>) {
    if (
      !previewResult ||
      previewResult.raw_screen_x == null ||
      previewResult.raw_screen_y == null ||
      previewResult.window_left == null ||
      previewResult.window_top == null ||
      !previewResult.window_width ||
      !previewResult.window_height
    ) {
      return;
    }

    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;

    const xRatio = clampNumber((e.clientX - rect.left) / rect.width, 0, 1);
    const yRatio = clampNumber((e.clientY - rect.top) / rect.height, 0, 1);
    const screenX = Math.round(previewResult.window_left + xRatio * previewResult.window_width);
    const screenY = Math.round(previewResult.window_top + yRatio * previewResult.window_height);
    if (draft.matchType === "point") {
      const pointValue = `window:${Math.round(screenX - previewResult.window_left)},${Math.round(screenY - previewResult.window_top)}`;
      setDraft((d) => ({ ...d, matchValue: pointValue, offsetX: 0, offsetY: 0 }));
      setPreviewResult((r) =>
        r
          ? {
              ...r,
              raw_screen_x: screenX,
              raw_screen_y: screenY,
              offset_x: 0,
              offset_y: 0,
              screen_x: screenX,
              screen_y: screenY,
            }
          : r
      );
      setFeedback(`准星 ${pointValue} · 最终 (${screenX}, ${screenY})`);
      return;
    }

    const offsetX = screenX - previewResult.raw_screen_x;
    const offsetY = screenY - previewResult.raw_screen_y;

    setDraft((d) => ({ ...d, offsetX, offsetY }));
    setPreviewResult((r) =>
      r
        ? {
            ...r,
            offset_x: offsetX,
            offset_y: offsetY,
            screen_x: screenX,
            screen_y: screenY,
          }
        : r
    );
    setFeedback(` (${screenX}, ${screenY})?${offsetX}, ${offsetY}`);
  }

  function stepForExecution(step: AutomationStep): AutomationStep {
    if (step.id !== selectedId) return step;
    if (draft.action !== "closeWindow" && !draft.matchValue.trim()) return step;
    return {
      ...step,
      name: draft.name.trim() || step.name,
      action: draft.action,
      windowTitle: draft.windowTitle.trim(),
      matchType: draft.matchType,
      matchValue: draft.matchValue.trim(),
      colorTolerance: draft.colorTolerance,
      click: draft.action === "closeWindow" ? false : draft.click || draft.action === "inputText",
      offsetX: draft.matchType === "point" ? 0 : draft.offsetX,
      offsetY: draft.matchType === "point" ? 0 : draft.offsetY,
      inputMode: draft.inputMode,
      inputText: draft.inputText.trim(),
    };
  }

  async function invokeStepRun(step: AutomationStep) {
    if (step.action === "closeWindow") {
      return invoke<VisualTargetResult>("close_target_window_cmd", {
        windowTitle: step.windowTitle || "WeGame",
      });
    }
    return invoke<VisualTargetResult>("run_visual_target_cmd", {
      action: step.action ?? "click",
      matchType: step.matchType,
      matchValue: step.matchValue,
      windowTitle: step.windowTitle,
      colorTolerance: step.colorTolerance,
      click: step.click || step.action === "inputText",
      dryRun: false,
      offsetX: step.offsetX,
      offsetY: step.offsetY,
      inputMode: step.inputMode ?? "installBase",
      inputText: step.inputText ?? "",
    });
  }

  async function waitBeforeStep(step: AutomationStep, label?: string) {
    if (step.timeAfterStep || step.delayMs <= 0) return;
    const waitLabel = formatDelay(step.delayMs, delayUnitOf(step));
    beginStepTimer(`等待 · ${step.name}`);
    if (label) {
      setLog((prev) => [...prev, `${label} 等待 ${waitLabel} 后执行「${step.name}」`]);
    } else {
      setFeedback(`等待 ${waitLabel} 后执行「${step.name}」`);
    }
    await new Promise((resolve) => setTimeout(resolve, step.delayMs));
    endStepTimer();
  }

  async function runOneStep(step: AutomationStep) {
    const exec = stepForExecution(step);
    if (exec.action !== "closeWindow" && !exec.matchValue.trim()) {
      setFeedback(exec.matchType === "point" ? "请先用准星选择坐标" : "请填写匹配文字或颜色");
      return;
    }
    setBusy(true);
    setRunningStepId(step.id);
    try {
      await waitBeforeStep(exec);
      beginStepTimer(exec.name);
      setFeedback(`执行「${exec.name}」`);
      const stepStart = Date.now();
      const r = await invokeStepRun(exec);
      const pos = r.screen_x != null && r.screen_y != null ? ` (${r.screen_x}, ${r.screen_y})` : "";
      const took = formatDurationShort(Date.now() - stepStart);
      if (exec.timeAfterStep && r.success) {
        beginStepTimer(exec.name);
        setAwaitingPhaseStep(exec);
        if (exec.delayMs > 0) {
          setFeedback(`${r.success ? "✓" : "✗"} ${r.message}${pos} · 等待 ${formatDelay(exec.delayMs, delayUnitOf(exec))}`);
          window.setTimeout(() => finishPhaseTimer(exec), exec.delayMs);
        } else {
          setFeedback(`${r.success ? "✓" : "✗"} ${r.message}${pos} · 计时中，完成后点「完成」`);
        }
      } else {
        endStepTimer();
        setRunningStepId(null);
        setFeedback(`${r.success ? "✓" : "✗"} ${r.message}${pos} · ${took}`);
      }
    } catch (e) {
      endStepTimer();
      setRunningStepId(null);
      setFeedback(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runAll() {
    const runnableSteps = steps
      .map((step, index) => ({ step, index }))
      .filter(({ step }) => step.enabled !== false);
    if (runnableSteps.length === 0) {
      setFeedback("");
      return;
    }
    resetExecutionTimer();
    beginTotalTimer();
    setRunning(true);
    setLog([`开始执行 (${runnableSteps.length}/${steps.length} 步)`]);

    for (let i = 0; i < runnableSteps.length; i++) {
      const { step: sourceStep, index: originalIndex } = runnableSteps[i];
      const step = stepForExecution(sourceStep);
      setRunningStepId(sourceStep.id);
      let stepSucceeded = false;
      let stepStart = Date.now();
      try {
        await waitBeforeStep(step, `[${originalIndex + 1}]`);
        beginStepTimer(step.name);
        setLog((prev) => [...prev, `[${i + 1}/${runnableSteps.length}] 执行「${step.name}」`]);
        stepStart = Date.now();
        const r = await invokeStepRun(step);
        stepSucceeded = r.success;
        const pos = r.screen_x != null && r.screen_y != null ? ` (${r.screen_x}, ${r.screen_y})` : "";
        const took = formatDurationShort(Date.now() - stepStart);
        const icon = r.success ? "✓" : "✗";
        setLog((prev) => {
          const next = [...prev];
          next[next.length - 1] = `[${i + 1}/${runnableSteps.length}] ${icon} ${step.name}${pos} · ${took} · ${r.message}`;
          return next;
        });
      } catch (e) {
        const took = formatDurationShort(Date.now() - stepStart);
        setLog((prev) => {
          const next = [...prev];
          next[next.length - 1] = `[${i + 1}/${runnableSteps.length}] ✗ ${step.name} · ${took} · ${e}`;
          return next;
        });
        endStepTimer();
        setRunningStepId(null);
        setRunning(false);
        return;
      }

      if (step.timeAfterStep && stepSucceeded) {
        beginStepTimer(step.name);
        setAwaitingPhaseStep(step);
        const waitLabel = formatDelay(step.delayMs, delayUnitOf(step));
        setLog((prev) => [
          ...prev,
          step.delayMs > 0
            ? `[${i + 1}/${runnableSteps.length}] 计时中「${step.name}」· 等待 ${waitLabel} 后点「完成」`
            : `[${i + 1}/${runnableSteps.length}] 计时中「${step.name}」· 从现在计时`
        ]);
        await new Promise<void>((resolve) => {
          let finished = false;
          let timerId: number | null = null;
          const finishOnce = () => {
            if (finished) return;
            finished = true;
            if (timerId !== null) window.clearTimeout(timerId);
            phaseCompleteRef.current = null;
            resolve();
          };
          phaseCompleteRef.current = () => {
            finishPhaseTimer(step);
            finishOnce();
          };
          if (step.delayMs > 0) {
            timerId = window.setTimeout(() => {
              finishPhaseTimer(step);
              finishOnce();
            }, step.delayMs);
          }
        });
        continue;
      }

      endStepTimer();
      setRunningStepId(null);

      if (i + 1 >= runnableSteps.length) break;
    }

    setRunningStepId(null);
    setLog((prev) => [...prev, "全部步骤执行完成"]);
    setRunning(false);
  }

  const markerStyle = previewMarkerStyle();

  return (
    <div className="page auto-page">
      <header className="page-head">
        <div>
          <p className="page-kicker">模拟点击 · 安装器自动化</p>
          <h2>自动化</h2>
          <p className="page-sub">定位文字/颜色/准星，按步骤模拟点击完成安装</p>
        </div>
      </header>

      <div className="automation-brief">
        <div className="brief-cell">
          <span className="brief-label">运行模式</span><strong>{running ? "执行中" : busy ? "调试中" : "待命"}</strong>
        </div>
        <div className="brief-cell">
          <span className="brief-label">步骤数</span><strong>{runnableStepCount}//{steps.length}</strong>
        </div>
        <div className="brief-cell"><span className="brief-label">推荐路径</span><strong>模拟点击安装</strong>
        </div>
        <div className="brief-cell">
          <span className="brief-label">权限</span><strong>{isElevated === null ? "检测中" : isElevated ? "管理员" : "普通"}</strong>
        </div>
        <div className={`brief-cell timer-cell${timerRunning ? " is-running" : ""}`}>
          <span className="brief-label">总计时</span><strong>{formatDuration(totalElapsedMs)}</strong>
        </div>
        <div className={`brief-cell timer-cell${phaseTimerActive ? " is-phase" : ""}${stepStartedAt !== null ? " is-running" : ""}`}>
          <span className="brief-label">{stepTimerName || "环节计时"}</span>
          <strong>{stepStartedAt !== null ? formatDuration(stepElapsedMs) : stepElapsedMs > 0 ? formatDuration(stepElapsedMs) : ""}</strong>
        </div>
        {awaitingPhaseStep ? (
          <button className="btn btn-sm btn-primary" type="button" onClick={completePhaseTimer}>完成</button>
        ) : timerRunning ? (
          <button className="btn btn-sm" type="button" onClick={stopExecutionTimer}>停止计时</button>
        ) : (
          <button className="btn btn-sm" type="button" onClick={resetExecutionTimer} disabled={totalElapsedMs === 0 && stepElapsedMs === 0}>清零计时</button>
        )}
        {isElevated === false && (
          <button className="btn btn-sm btn-primary" type="button" onClick={() => void restartAsAdmin()} disabled={busy || running}>管理员重启</button>
        )}
      </div>

      <section className="auto-panel auto-template-panel">
        <div className="auto-template-grid">
          <label className="auto-template-field"><span>当前模板</span><select
              className="path-input"
              value={activeTemplateId}
              onChange={(e) => void switchTemplate(e.target.value)}
              disabled={busy || running || templates.length === 0}
            >
              {templates.map((template) => (
                <option key={template.id} value={template.id}>
                  {template.name}
                </option>
              ))}
            </select>
          </label>
          <label className="auto-template-field"><span>模板名称</span><input className="path-input" value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
              placeholder="例如：WeGame 标准安装"
              disabled={busy || running}
            />
          </label>
          <div className="auto-template-actions">
            <button className="btn btn-sm btn-primary" type="button" onClick={() => void saveCurrentAsTemplate()} disabled={busy || running}>保存模板</button>
            <button className="btn btn-sm btn-danger" type="button" onClick={() => void deleteActiveTemplate()} disabled={busy || running || templates.length <= 1}>删除模板</button>
          </div>
        </div>
      </section>

      {/* ?*/}
      <section className="auto-panel auto-panel-main">
        <div className="auto-panel-head">
          <h3>步骤</h3>
          <div className="auto-panel-actions">
            <button className="btn btn-sm btn-ghost" type="button" onClick={openWeGameInstaller} disabled={busy || running}>打开 WeGame 安装包</button>
            <button className="btn btn-sm btn-primary" type="button" onClick={addStep} disabled={busy || running}>+ 添加步骤</button>
            <button
              className="btn btn-sm btn-primary"
              type="button"
              onClick={() => void runAll()}
              disabled={busy || running || runnableStepCount === 0}>执行全部{runnableStepCount ? ` (${runnableStepCount})` : ""}
            </button>
          </div>
        </div>

        {steps.length > 0 ? (
          <div className={`auto-table-wrap${dragStepId && !dragSettling ? " is-step-dragging" : ""}${dragSettling ? " is-step-settling" : ""}`}>
            <table className={`auto-table${isDragActive ? " is-reordering" : ""}`}>
            <thead>
              <tr>
                <th className="chain-col-drag" aria-label="按住拖动排序" />
                <th className="chain-col-run">运行</th>
                <th className="chain-col-time">计时</th>
                <th className="chain-col-num">#</th><th>名称</th><th>类型</th><th>匹配</th>
                <th className="chain-col-delay">触发前等待</th>
                <th className="chain-col-actions">操作</th>
              </tr>
            </thead>
            <tbody>
              {steps.map((step, idx) => {
                const isDraggingRow = dragStepId === step.id || (dragSettling && dragFromIdx === idx);
                const shift =
                  isDragActive && dragFromIdx !== null && dragOverIdx !== null && !isDraggingRow
                    ? getStepRowShift(idx, dragFromIdx, dragOverIdx, rowHeight)
                    : 0;
                return (
                <tr
                  key={step.id}
                  data-step-idx={idx}
                  className={`${step.id === selectedId ? "is-selected" : ""}${runningStepId === step.id ? " is-running-step" : ""}${step.enabled === false ? " is-disabled-step" : ""}${dragOverIdx === idx && dragStepId && dragOverIdx !== dragFromIdx ? " is-drag-over" : ""}${isDraggingRow ? " is-dragging" : ""}`}
                  style={isDragActive && !isDraggingRow ? { transform: `translateY(${shift}px)` } : undefined}
                  onClick={() => selectStep(step)}
                >
                  <td
                    className="chain-col-drag"
                    onClick={(e) => e.stopPropagation()}
                    onPointerDown={(e) => beginStepDrag(e, idx)}
                  >
                    <span className="step-drag-handle" title="按住拖动排序">
                      拖动
                    </span>
                  </td>
                  <td className="chain-col-run" onClick={(e) => e.stopPropagation()}>
                    <label className="step-run-toggle" title="关闭后，执行全部会跳过这一行">
                      <input
                        type="checkbox"
                        checked={step.enabled !== false}
                        onChange={(e) => setStepEnabled(step.id, e.target.checked)}
                        disabled={running}
                      />
                    </label>
                  </td>
                  <td className="chain-col-time" onClick={(e) => e.stopPropagation()}>
                    <button
                      type="button"
                      className={`step-timed-toggle${step.timeAfterStep ? " is-on" : ""}`}
                      onClick={() => setStepTimeAfter(step.id, !step.timeAfterStep)}
                      disabled={running || step.enabled === false}
                      title="开启后，本步从开始执行到点「完成」单独计时"
                    >
                      计时
                    </button>
                    {runningStepId === step.id && stepStartedAt !== null ? (
                      <span className="step-timed-live">{formatDuration(stepElapsedMs)}</span>
                    ) : (step.lastMeasuredMs ?? 0) > 0 ? (
                      <span className="step-timed-last">{formatDurationShort(step.lastMeasuredMs ?? 0)}</span>
                    ) : null}
                  </td>
                  <td className="chain-col-num">{idx + 1}</td>
                  <td className="auto-rule-name">{step.name}</td>
                  <td>{stepTypeLabel(step)}</td>
                  <td className="auto-match-val">{step.matchValue || ""}</td>
                  <td className="chain-col-delay" onClick={(e) => e.stopPropagation()}>
                    <label className="chain-delay-inline">
                      <input
                        type="number"
                        className="chain-delay-input"
                        min={0}
                        step={delayUnitOf(step) === "s" ? 0.1 : 100}
                        value={delayDisplayValue(step)}
                        onChange={(e) => void setStepDelayValue(step.id, Number(e.target.value) || 0)}
                        disabled={running}
                      />
                      <select
                        className="chain-delay-unit"
                        value={delayUnitOf(step)}
                        onChange={(e) => setStepDelayUnit(step.id, e.target.value === "s" ? "s" : "ms")}
                        disabled={running}
                      >
                        <option value="ms">ms</option>
                        <option value="s">s</option>
                      </select>
                    </label>
                  </td>
                  <td className="auto-rule-actions chain-col-actions" onClick={(e) => e.stopPropagation()}>
                    <button type="button" className="btn-link" onClick={() => selectStep(step)} disabled={running}>编辑</button>
                    <button type="button" className="btn-link" onClick={() => void runOneStep(step)} disabled={busy || running}>执行</button>
                    <button type="button" className="btn-link btn-link-danger" onClick={() => void removeStep(step.id)} disabled={running}>删</button>
                  </td>
                </tr>
                );
              })}
            </tbody>
          </table>
          {dragStepId && draggedStep && dragFromIdx !== null && (
            <div
              className={`step-drag-ghost${dragSettling ? " is-leaving" : ""}`}
              style={{
                width: ghostWidth > 0 ? ghostWidth : undefined,
                transform: `translate3d(${dragPointer.x - dragOffsetRef.current.x}px, ${dragPointer.y - dragOffsetRef.current.y}px, 0) scale(${dragSettling ? 0.96 : 1.02})`,
                opacity: dragSettling ? 0 : 1,
                filter: dragSettling ? "blur(2px)" : undefined,
                transition: dragSettling
                  ? "opacity 0.28s cubic-bezier(0.22, 1, 0.36, 1), transform 0.28s cubic-bezier(0.22, 1, 0.36, 1), filter 0.28s cubic-bezier(0.22, 1, 0.36, 1)"
                  : undefined,
              }}
            >
              <span className="step-drag-ghost-handle"></span>
              <span className="step-drag-ghost-num">{dragFromIdx + 1}</span>
              <span className="step-drag-ghost-name">{draggedStep.name}</span>
              <span className="step-drag-ghost-type">{stepTypeLabel(draggedStep)}</span>
            </div>
          )}
          </div>
        ) : (
          <div className="empty-architecture">
            <strong>还没有自动化链路</strong><span>添加步骤后点「保存模板」，以后就可以在上方切换使用。</span>
            <button className="btn btn-sm btn-primary" type="button" onClick={addStep} disabled={busy || running}>+ 添加步骤</button>
          </div>
        )}

        {log.length > 0 && (
          <div className="chain-log">
            {log.map((entry, i) => (
              <div key={i} className={`chain-log-entry${entry.includes("✗") ? " is-error" : entry.includes("✓") ? " is-ok" : ""}`}>{entry}</div>
            ))}
          </div>
        )}
      </section>

      {/* ?*/}
      <section className="auto-panel auto-panel-locator">
        <h3>定位器<span className="auto-panel-sub">{selected ? ` · 单次调试：${selected.name}` : ""}</span>
        </h3>

        {!selected ? (
          <p className="auto-empty auto-empty-dim">选中上方某个步骤，或先添加步骤，再在这里试匹配和点击。</p>
        ) : (
          <>
            <div className="auto-form-row">
              <input
                className="auto-input auto-input-sm"
                placeholder="步骤名称"
                value={draft.name}
                onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
                spellCheck={false}
                disabled={busy || running}
              />
            </div>
            <div className="auto-form-row">
              <button
                type="button"
                className={`btn btn-sm${draft.action === "click" ? " btn-primary" : ""}`}
                onClick={() => setDraft((d) => ({ ...d, action: "click", click: true }))}
                disabled={busy || running}
              >
                左键点击
              </button>
              <button
                type="button"
                className={`btn btn-sm${draft.action === "inputText" ? " btn-primary" : ""}`}
                onClick={() => setDraft((d) => ({ ...d, action: "inputText", click: true }))}
                disabled={busy || running}
              >
                输入路径
              </button>
              <button
                type="button"
                className={`btn btn-sm${draft.action === "closeWindow" ? " btn-primary" : ""}`}
                onClick={() => setDraft((d) => ({ ...d, action: "closeWindow", click: false }))}
                disabled={busy || running}
              >
                关闭窗口
              </button>
              <span className="auto-offset-hint">输入路径会先点中目标；关闭窗口会先正常关闭，失败再强制结束。</span>
            </div>
            {draft.action !== "closeWindow" && (
            <>
            <div className="auto-form-row">
              <button
                type="button"
                className={`btn btn-sm${draft.matchType === "text" ? " btn-primary" : ""}`}
                onClick={() => setDraft((d) => ({ ...d, matchType: "text" }))}
                disabled={busy || running}
              >
                文字
              </button>
              <button
                type="button"
                className={`btn btn-sm${draft.matchType === "color" ? " btn-primary" : ""}`}
                onClick={() => setDraft((d) => ({ ...d, matchType: "color" }))}
                disabled={busy || running}
              >
                颜色
              </button>
              <button
                type="button"
                className={`btn btn-sm${draft.matchType === "point" ? " btn-primary" : ""}`}
                onClick={choosePointMode}
                disabled={busy || running}
              >
                准星
              </button>
              <button
                type="button"
                className="btn btn-sm"
                onClick={useCurrentCrosshair}
                disabled={busy || running || !previewResult}
              >
                取当前准星
              </button>
              <input
                className="auto-input auto-input-grow"
                placeholder={draft.matchType === "point" ? "点预览后拖动准星" : draft.matchType === "text" ? "匹配文字" : "#RRGGBB"}
                value={draft.matchValue}
                onChange={(e) => setDraft((d) => ({ ...d, matchValue: e.target.value }))}
                spellCheck={false}
                disabled={busy || running}
              />
              <label className="auto-check-label">
                <input
                  type="checkbox"
                  checked={draft.click}
                  onChange={(e) => setDraft((d) => ({ ...d, click: e.target.checked }))}
                  disabled={busy || running}
                />
                点击
              </label>
            </div>
            {draft.matchType !== "point" ? (
            <div className="auto-form-row">
              <label className="auto-offset-label">
                偏移 X
                <input
                  type="number"
                  className="chain-delay-input"
                  step={1}
                  value={draft.offsetX}
                  onChange={(e) => setDraft((d) => ({ ...d, offsetX: Number(e.target.value) || 0 }))}
                  disabled={busy || running}
                />
              </label>
              <label className="auto-offset-label">
                偏移 Y
                <input
                  type="number"
                  className="chain-delay-input"
                  step={1}
                  value={draft.offsetY}
                  onChange={(e) => setDraft((d) => ({ ...d, offsetY: Number(e.target.value) || 0 }))}
                  disabled={busy || running}
                />
              </label>
              <span className="auto-offset-hint">相对识别中心点，单位像素；也可以直接拖动预览准星。</span>
            </div>
            ) : (
              <div className="auto-form-row">
                <span className="auto-offset-hint"></span>
              </div>
            )}
            {draft.matchType === "color" && (
              <div className="auto-form-row">
                <input
                  type="color"
                  className="auto-color-picker"
                  value={draft.matchValue.startsWith("#") ? draft.matchValue : "#ff5500"}
                  onChange={(e) => setDraft((d) => ({ ...d, matchValue: e.target.value }))}
                  disabled={busy || running}
                />
                <button className="btn btn-sm" type="button" onClick={() => void pickColorFromMouse()} disabled={busy || running}>
                  鼠标拾色
                </button>
                <label className="auto-tolerance">
                  容差
                  <input
                    type="number"
                    min={1}
                    max={80}
                    value={draft.colorTolerance}
                    onChange={(e) => setDraft((d) => ({ ...d, colorTolerance: Number(e.target.value) || 24 }))}
                    disabled={busy || running}
                  />
                </label>
              </div>
            )}
            </>
            )}
            {draft.action === "inputText" && (
              <div className="auto-form-row">
                <label className="auto-check-label">
                  <input
                    type="radio"
                    checked={draft.inputMode === "installBase"} onChange={() => setDraft((d) => ({ ...d, inputMode: "installBase" }))} disabled={busy || running} />使用设置页安装目录</label>
                <label className="auto-check-label">
                  <input
                    type="radio"
                    checked={draft.inputMode === "custom"} onChange={() => setDraft((d) => ({ ...d, inputMode: "custom" }))} disabled={busy || running} />手动路径</label>
                <input
                  className="auto-input auto-input-grow"
                  placeholder="例如 D:\\Games\\WeGame"
                  value={draft.inputText}
                  onChange={(e) => setDraft((d) => ({ ...d, inputText: e.target.value }))}
                  spellCheck={false}
                  disabled={busy || running || draft.inputMode !== "custom"}
                />
              </div>
            )}
            <div className="auto-form-row">
              <input
                className="auto-input auto-input-window"
                placeholder="窗口标题" value={draft.windowTitle}
                onChange={(e) => setDraft((d) => ({ ...d, windowTitle: e.target.value }))}
                spellCheck={false}
                disabled={busy || running}
              />
              <button className="btn btn-sm" type="button" onClick={() => void runLocator(true)} disabled={busy || running}>预览坐标</button>
              <button className="btn btn-sm" type="button" onClick={() => void runLocator(false)} disabled={busy || running}>执行一次</button>
              <button
                className="btn btn-sm"
                type="button"
                onClick={() => void closeTargetWindow(draft.windowTitle.trim() || "WeGame")} disabled={busy || running}>关闭目标</button>
              <button className="btn btn-sm btn-primary" type="button" onClick={() => void saveDraftToStep()} disabled={busy || running}>保存</button>
            </div>
            {feedback && <p className="auto-feedback">{feedback}</p>}
            {previewResult && (
              <div className="auto-coordinates">
                <div>
                  <span>{draft.matchType === "point" ? "准星坐标" : draft.matchType === "color" ? "原始颜色" : "原始 OCR"}</span>
                  <strong>
                    {previewResult.raw_screen_x ?? ""}, {previewResult.raw_screen_y ?? ""}
                  </strong>
                </div>
                <div>
                  <span>偏移</span>
                  <strong>
                    {previewResult.offset_x ?? 0}, {previewResult.offset_y ?? 0}
                  </strong>
                </div>
                <div>
                  <span>最终坐标</span>
                  <strong>
                    {previewResult.screen_x ?? ""}, {previewResult.screen_y ?? ""}
                  </strong>
                </div>
              </div>
            )}
            {previewImage && (
              <div className="auto-preview">
                <div className="auto-preview-head">
                  <strong>截图预览</strong>
                  <span>拖动准星校准最终坐标</span>
                </div>
                <div
                  className="auto-preview-stage"
                  onPointerDown={(e) => {
                    e.currentTarget.setPointerCapture(e.pointerId);
                    updatePreviewPoint(e);
                  }}
                  onPointerMove={(e) => {
                    if (e.buttons === 1) updatePreviewPoint(e);
                  }}
                >
                  <img src={previewImage} alt="OCR 预览" draggable={false} />
                  {markerStyle && (
                    <span className="auto-preview-crosshair" style={markerStyle} aria-label="准星">
                      <span />
                    </span>
                  )}
                </div>
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}

/* ?
   MAIN APP
   ?*/

function App() {
  const [nav, setNav] = useState<NavId>("library");
  const [software, setSoftware] = useState<SoftwareInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState<Record<string, InstallStatus>>({});
  const [installed, setInstalled] = useState<Record<string, boolean>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batchBusy, setBatchBusy] = useState(false);
  const [pathSettings, setPathSettings] = useState<InstallPathSettings | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [appPaths, setAppPaths] = useState<Record<string, AppInstallPaths>>({});
  const [packageCache, setPackageCache] = useState<Record<string, PackageCacheInfo>>({});
  const [librarySourceFilter, setLibrarySourceFilter] = useState<LibrarySourceFilter>("all");
  const [libraryDetailId, setLibraryDetailId] = useState<string | null>(null);
  const [showCustomSource, setShowCustomSource] = useState(false);
  const [editingCustomSourceId, setEditingCustomSourceId] = useState<string | null>(null);
  const [customSourceForm, setCustomSourceForm] = useState<CustomSourceForm>(DEFAULT_CUSTOM_SOURCE_FORM);
  const [scanCandidates, setScanCandidates] = useState<DownloadCandidate[]>([]);
  const [scanBusy, setScanBusy] = useState(false);
  const [scanMessage, setScanMessage] = useState("");
  const [wingetStatus, setWingetStatus] = useState<WingetStatus | null>(null);
  const [wingetChecking, setWingetChecking] = useState(false);
  const missingIconRefreshStarted = useRef(false);
  const activeInstallerLaunches = useRef(new Set<string>());

  const failedItems = software.filter((sw) => sw.latest_version.startsWith("查询失败"));
  const selectable = software.filter((sw) => sw.portable);
  const libraryAvailableItems = software.filter((sw) => sw.portable || isMicrosoftStoreApp(sw));
  const visibleLibraryBaseItems = software.filter((sw) => sw.portable || isMicrosoftStoreApp(sw) || sw.latest_version.startsWith("查询失败"));
  const libraryItems = visibleLibraryBaseItems.filter((sw) => {
    if (librarySourceFilter === "all") return true;
    return softwareSourceKind(sw) === librarySourceFilter;
  });
  const githubLibraryItems = libraryAvailableItems.filter((sw) => softwareSourceKind(sw) === "github");
  const officialLibraryItems = libraryAvailableItems.filter((sw) => softwareSourceKind(sw) === "official");
  const storeLibraryItems = libraryAvailableItems.filter((sw) => softwareSourceKind(sw) === "store");
  const portableItems = selectable.filter((sw) => sw.install_kind === "portable");
  const downloadOnlyItems = selectable.filter((sw) => sw.install_kind === "download");
  const installerItems = software.filter((sw) => sw.install_kind === "installer" && sw.portable);
  const cachedItems = software.filter((sw) => packageCache[sw.id]?.cached);
  const cachedBytes = cachedItems.reduce((sum, sw) => sum + (packageCache[sw.id]?.size || sw.portable?.size || 0), 0);
  const notInstalledCount = libraryAvailableItems.filter((sw) => !isDownloadOnly(sw) && !installed[sw.id]).length;
  const sourceHealth = failedItems.length ? `${failedItems.length} 个源异常` : loading ? "同步中" : software.length ? "正常" : "待检测";
  const isCustomPath =
    pathSettings && pathSettings.current_base.replace(/\\+$/, "") !== pathSettings.default_base.replace(/\\+$/, "");

  const selectedOneId = selected.size === 1 ? [...selected][0] : null;

  useEffect(() => {
    const blockContextMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", blockContextMenu);
    return () => document.removeEventListener("contextmenu", blockContextMenu);
  }, []);

  function isItemBusy(id: string) {
    const st = status[id];
    return st?.state === "downloading" || st?.state === "paused" || st?.state === "cancelling" || st?.state === "uninstalling" || st?.state === "installing" || st?.state === "checking";
  }

  function isDownloadControllable(id: string) {
    const state = status[id]?.state;
    return state === "downloading" || state === "paused" || state === "cancelling";
  }

  const installableIds = [...selected].filter((id) => {
    const sw = software.find((s) => s.id === id);
    return sw?.portable && sw.install_kind === "portable" && !isItemBusy(id);
  });
  const selectedInstallableItems = installableIds
    .map((id) => software.find((s) => s.id === id))
    .filter((sw): sw is SoftwareInfo => !!sw && sw.install_kind === "portable");
  const primaryActionLabel = "安装";

  const uninstallableItems = [...selected]
    .map((id) => software.find((s) => s.id === id))
    .filter((sw): sw is SoftwareInfo => !!sw && !!installed[sw.id] && !isItemBusy(sw.id));

  async function refreshAppPaths(list: SoftwareInfo[]) {
    const entries = await Promise.all(
      list.filter((sw) => sw.portable).map(async (sw) => {
        const paths = await invoke<AppInstallPaths>("get_app_install_paths_cmd", {
          id: sw.id, displayName: sw.display_name, version: sw.latest_version, fileName: sw.portable!.name,
        });
        return [sw.id, paths] as const;
      })
    );
    setAppPaths(Object.fromEntries(entries));
  }

  async function refreshPackageCache(list: SoftwareInfo[], replace = true) {
    const entries = await Promise.all(
      list.filter((sw) => sw.portable).map(async (sw) => {
        const info = await invoke<PackageCacheInfo>("get_package_cache_info_cmd", {
          id: sw.id, version: sw.latest_version, fileName: sw.portable!.name, expectedSize: sw.portable!.size,
        });
        return [sw.id, info] as const;
      })
    );
    const next = Object.fromEntries(entries);
    setPackageCache((prev) => (replace ? next : { ...prev, ...next }));
  }

  async function loadPathSettings() {
    const settings = await invoke<InstallPathSettings>("get_install_paths_cmd");
    setPathSettings(settings);
    setPathInput(settings.current_base);
  }

  async function refreshInstalled(list: SoftwareInfo[]) {
    const entries = await Promise.all(
      list.filter((sw) => !isDownloadOnly(sw)).map(async (sw) => {
        const ok = await invoke<boolean>("is_software_installed", { id: sw.id });
        return [sw.id, ok] as const;
      })
    );
    setInstalled((prev) => ({ ...prev, ...Object.fromEntries(entries) }));
  }

  async function detectInstalledNow(list: SoftwareInfo[] = software) {
    const targets = list.filter((sw) => (sw.portable || isMicrosoftStoreApp(sw)) && !isDownloadOnly(sw));
    if (!targets.length) return;

    setStatus((prev) => {
      const next = { ...prev };
      for (const sw of targets) {
        next[sw.id] = { state: "checking", percent: 30, message: "正在检测…" };
      }
      return next;
    });

    try {
      const entries = await Promise.all(
        targets.map(async (sw) => {
          const ok = await invoke<boolean>("is_software_installed", { id: sw.id });
          return [sw, ok] as const;
        })
      );

      setInstalled((prev) => {
        const next = { ...prev };
        for (const [sw, ok] of entries) next[sw.id] = ok;
        return next;
      });

      setStatus((prev) => {
        const next = { ...prev };
        for (const [sw, ok] of entries) {
          next[sw.id] = {
            state: "done",
            percent: 100,
            message: ok ? "已安装" : "未安装",
          };
        }
        return next;
      });
    } catch (e) {
      setStatus((prev) => {
        const next = { ...prev };
        for (const sw of targets) {
          next[sw.id] = { state: "error", percent: 0, message: String(e) };
        }
        return next;
      });
    }
  }

  const loadAll = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [swRes] = await Promise.all([
        invoke<SoftwareInfo[]>("fetch_all_software"),
        loadPathSettings(),
      ]);
      setSoftware(swRes);
      await refreshInstalled(swRes);
      await refreshAppPaths(swRes);
      await refreshPackageCache(swRes);
      setSelected((prev) => {
        const valid = new Set(swRes.filter((sw) => sw.portable).map((sw) => sw.id));
        return new Set([...prev].filter((id) => valid.has(id)));
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const refreshWingetStatus = useCallback(async () => {
    setWingetChecking(true);
    try {
      setWingetStatus(await invoke<WingetStatus>("get_winget_status_cmd"));
    } catch (e) {
      setWingetStatus({ available: false, version: "", message: String(e) });
    } finally {
      setWingetChecking(false);
    }
  }, []);

  const openWingetTerminal = useCallback(async () => {
    try {
      await invoke("open_winget_terminal_cmd");
    } catch (e) {
      setWingetStatus({ available: false, version: "", message: String(e) });
    }
  }, []);

  const searchWingetPackages = useCallback((query: string) => (
    invoke<WingetSearchResult[]>("search_winget_packages_cmd", { query })
  ), []);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("download-progress", (event) => {
      const p = event.payload;
      setStatus((prev) => {
        const current = prev[p.id];
        // 暂停和取消是用户刚刚明确做出的操作，不能被在途的最后一条进度事件覆盖。
        if (current?.state === "paused" || current?.state === "cancelling") return prev;
        return {
          ...prev,
          [p.id]: { state: "downloading", percent: p.percent, message: `${formatSize(p.downloaded)} / ${formatSize(p.total)}` },
        };
      });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  useEffect(() => { loadAll(); }, [loadAll]);

  useEffect(() => {
    if (missingIconRefreshStarted.current) return;
    if (!software.some((sw) => isUserSource(sw) && !sw.icon_path.trim())) return;

    missingIconRefreshStarted.current = true;
    void (async () => {
      const updatedIds = await invoke<string[]>("fetch_missing_custom_software_icons").catch(() => []);
      if (!updatedIds.length) return;
      const refreshed = await invoke<SoftwareInfo[]>("fetch_all_software").catch(() => null);
      if (refreshed) setSoftware(refreshed);
    })();
  }, [software]);

  useEffect(() => {
    void refreshWingetStatus();
  }, [refreshWingetStatus]);

  useEffect(() => {
    if (nav !== "library") setLibraryDetailId(null);
  }, [nav]);

  function openLibraryDetail(id: string) {
    setLibraryDetailId(id);
    setLibrarySourceFilter("all");
    setNav("library");
  }


  function closeLibraryDetail() {
    setLibraryDetailId(null);
  }

  const installBasePath = (pathSettings?.current_base ?? pathInput).replace(/[\\/]+$/, "");

  async function openCachedPackage(sw: SoftwareInfo, allowExistingBusy = false) {
    const info = packageCache[sw.id];
    if (!info?.cached || !info.path) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: "安装包未缓存，请先下载" } }));
      return;
    }
    if ((!allowExistingBusy && isItemBusy(sw.id)) || activeInstallerLaunches.current.has(sw.id)) return;

    activeInstallerLaunches.current.add(sw.id);
    try {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "installing", percent: 85, message: "正在启动安装器…" } }));
      const launch = await invoke<PackageLaunchResult>("open_cached_package_cmd", { path: info.path });
      if (!launch.tracked) {
        setStatus((prev) => ({ ...prev, [sw.id]: { state: "done", percent: 100, message: launch.message } }));
        return;
      }

      setStatus((prev) => ({ ...prev, [sw.id]: { state: "installing", percent: 90, message: "安装器正在运行，请勿重复启动" } }));
      while (await invoke<boolean>("is_cached_installer_running_cmd", { path: info.path })) {
        await sleep(1000);
      }
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "done", percent: 100, message: "安装器已关闭，请点击“检测安装”确认结果" } }));
    } catch (e) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: String(e) } }));
    } finally {
      activeInstallerLaunches.current.delete(sw.id);
    }
  }

  async function runSilentInstaller(sw: SoftwareInfo) {
    const info = packageCache[sw.id];
    if (!canRunSilentInstall(sw) || !info?.cached || !info.path || isItemBusy(sw.id)) return;

    setStatus((prev) => ({ ...prev, [sw.id]: { state: "installing", percent: 60, message: "正在启动静默安装…" } }));
    try {
      const result = await invoke<{ message: string }>("run_silent_installer_cmd", {
        id: sw.id,
        path: info.path,
        arguments: sw.silent_install_args,
      });
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "done", percent: 100, message: result.message } }));
    } catch (e) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: String(e) } }));
    }
  }

  async function openFreshCachedPackage(sw: SoftwareInfo) {
    if (!sw.portable) return;
    const info = await invoke<PackageCacheInfo>("get_package_cache_info_cmd", {
      id: sw.id,
      version: sw.latest_version,
      fileName: sw.portable.name,
      expectedSize: sw.portable.size,
    });
    if (!info.cached || !info.path) {
      throw new Error("安装包缓存不存在，请重新下载");
    }
    await openCachedPackage(sw, true);
  }

  async function copyInstallDir(sw: SoftwareInfo) {
    const dir = `${installBasePath}\\${sw.id}`;
    try {
      await navigator.clipboard.writeText(dir);
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "done", percent: 100, message: `已复制路径 ${dir}` } }));
    } catch (e) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: String(e) } }));
    }
  }

  async function openMicrosoftStore(sw: SoftwareInfo) {
    const productId = microsoftStoreProductId(sw);
    if (!productId) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: "Microsoft Store 产品编号缺失" } }));
      return;
    }

    try {
      await invoke("open_microsoft_store_cmd", { productId });
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "done", percent: 100, message: "已打开 Microsoft Store" } }));
    } catch (e) {
      setStatus((prev) => ({ ...prev, [sw.id]: { state: "error", percent: 0, message: String(e) } }));
    }
  }

  async function uninstallInstaller(sw: SoftwareInfo) {
    const ok = window.confirm(`确定启动 ${sw.display_name} 卸载程序？`);
    if (!ok) return;
    await uninstallOne(sw);
  }

  async function savePath() {
    const settings = await invoke<InstallPathSettings>("set_install_paths_cmd", { path: pathInput });
    setPathSettings(settings);
    setPathInput(settings.current_base);
    await refreshAppPaths(software);
  }

  async function useDefaultPath() {
    const settings = await invoke<InstallPathSettings>("set_install_paths_cmd", { path: pathSettings?.default_base ?? "" });
    setPathSettings(settings);
    setPathInput(settings.current_base);
    await refreshAppPaths(software);
  }

  async function resetDefaultPath() {
    const settings = await invoke<InstallPathSettings>("reset_install_paths_cmd");
    setPathSettings(settings);
    setPathInput(settings.current_base);
    await refreshAppPaths(software);
  }

  async function pickFolder() {
    const picked = await open({ directory: true, multiple: false, title: "选择安装根目录" });
    if (typeof picked === "string") setPathInput(picked);
  }

  async function installOne(sw: SoftwareInfo) {
    if (!sw.portable) return;
    const id = sw.id;
    const cached = packageCache[id]?.cached;
    setStatus((prev) => ({ ...prev, [id]: { state: "downloading", percent: cached ? 100 : 0, message: cached ? "使用缓存" : "下载中…" } }));
    try {
      await invoke<{ message: string }>("install_software", {
        id, url: sw.portable.browser_download_url, fileName: sw.portable.name, version: sw.latest_version, expectedSize: sw.portable.size,
      });
      await refreshPackageCache([sw], false);
      await detectInstalledNow([sw]);
    } catch (e) {
      const cancelled = isDownloadCancelledError(e);
      setStatus((prev) => ({
        ...prev,
        [id]: {
          state: cancelled ? "cancelled" : "error",
          percent: 0,
          message: cancelled ? "已取消下载，临时文件已清除" : String(e),
        },
      }));
    }
  }

  function isDownloadCancelledError(error: unknown) {
    return String(error).includes("下载已取消");
  }

  async function cacheOne(sw: SoftwareInfo): Promise<boolean> {
    if (!sw.portable) return false;
    const id = sw.id;
    const cached = packageCache[id]?.cached;
    setStatus((prev) => ({ ...prev, [id]: { state: "downloading", percent: cached ? 100 : 0, message: cached ? "使用缓存" : "下载中…" } }));
    try {
      const result = await invoke<{ message: string }>("cache_software_package", {
        id, url: sw.portable.browser_download_url, fileName: sw.portable.name, version: sw.latest_version, expectedSize: sw.portable.size,
      });
      setStatus((prev) => ({ ...prev, [id]: { state: "done", percent: 100, message: result.message } }));
      await refreshPackageCache([sw], false);
      return true;
    } catch (e) {
      const cancelled = isDownloadCancelledError(e);
      setStatus((prev) => ({
        ...prev,
        [id]: {
          state: cancelled ? "cancelled" : "error",
          percent: 0,
          message: cancelled ? "已取消下载，临时文件已清除" : String(e),
        },
      }));
      return false;
    }
  }

  async function pauseDownload(sw: SoftwareInfo) {
    const current = status[sw.id];
    if (current?.state !== "downloading") return;

    try {
      await invoke("pause_download_cmd", { id: sw.id });
      setStatus((prev) => {
        const latest = prev[sw.id];
        if (!latest || latest.state !== "downloading") return prev;
        return { ...prev, [sw.id]: { ...latest, state: "paused", message: `已暂停 · ${latest.message}` } };
      });
    } catch (e) {
      setStatus((prev) => ({
        ...prev,
        [sw.id]: { ...(prev[sw.id] ?? current), state: "downloading", message: `暂停失败：${String(e)}` },
      }));
    }
  }

  async function resumeDownload(sw: SoftwareInfo) {
    const current = status[sw.id];
    if (current?.state !== "paused") return;

    try {
      await invoke("resume_download_cmd", { id: sw.id });
      setStatus((prev) => {
        const latest = prev[sw.id];
        if (!latest || latest.state !== "paused") return prev;
        return { ...prev, [sw.id]: { ...latest, state: "downloading", message: "继续下载…" } };
      });
    } catch (e) {
      setStatus((prev) => ({
        ...prev,
        [sw.id]: { ...(prev[sw.id] ?? current), state: "paused", message: `继续失败：${String(e)}` },
      }));
    }
  }

  async function cancelDownload(sw: SoftwareInfo) {
    const current = status[sw.id];
    if (current?.state !== "downloading" && current?.state !== "paused") return;

    try {
      await invoke("cancel_download_cmd", { id: sw.id });
      setStatus((prev) => {
        const latest = prev[sw.id];
        if (!latest || (latest.state !== "downloading" && latest.state !== "paused")) return prev;
        return { ...prev, [sw.id]: { ...latest, state: "cancelling", message: "正在取消下载…" } };
      });
    } catch (e) {
      setStatus((prev) => ({
        ...prev,
        [sw.id]: { ...(prev[sw.id] ?? current), message: `取消失败：${String(e)}` },
      }));
    }
  }

  function openNewCustomSourceModal() {
    setEditingCustomSourceId(null);
    setCustomSourceForm(DEFAULT_CUSTOM_SOURCE_FORM);
    setScanCandidates([]);
    setScanMessage("");
    setShowCustomSource(true);
  }

  async function openEditCustomSourceModal(sw: SoftwareInfo) {
    if (!isUserSource(sw)) return;
    try {
      const config = await invoke<CustomSourceForm>("get_custom_software", { id: sw.id });
      setEditingCustomSourceId(config.id);
      setScanCandidates([]);
      setScanMessage("");
      setCustomSourceForm({ ...DEFAULT_CUSTOM_SOURCE_FORM, ...config });
      setShowCustomSource(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveCustomSource(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const displayName = customSourceForm.display_name.trim();
    const sourceLink = customSourceForm.source_kind === "github"
      ? customSourceForm.repo
      : customSourceForm.download_url;
    const finalDisplayName = displayName
      || (customSourceForm.source_kind === "github" ? titleFromRepo(customSourceForm.repo) : "")
      || titleFromFileName(customSourceForm.file_name)
      || titleFromFileName(fileNameFromUrl(sourceLink))
      || "自定义软件";
    const form = {
      ...customSourceForm,
      id: editingCustomSourceId ?? generateCustomSourceId(finalDisplayName, software.map((sw) => sw.id)),
      display_name: finalDisplayName,
      repo: customSourceForm.source_kind === "github" ? (parseGithubRepo(customSourceForm.repo) || customSourceForm.repo.trim()) : customSourceForm.repo.trim(),
      asset_match: customSourceForm.asset_match.trim(),
      exe_match: customSourceForm.exe_match.trim(),
      download_url: customSourceForm.source_kind === "direct"
        ? normalizeHttpLink(customSourceForm.download_url)
        : customSourceForm.download_url.trim(),
      page_url: customSourceForm.page_url.trim(),
      version: customSourceForm.version.trim(),
      file_name: customSourceForm.file_name.trim(),
      silent_install_args: customSourceForm.silent_install_args.trim(),
    };

    try {
      setLoading(true);
      await invoke("add_custom_software", { config: form });
      setShowCustomSource(false);
      setEditingCustomSourceId(null);
      setCustomSourceForm(DEFAULT_CUSTOM_SOURCE_FORM);
      setScanCandidates([]);
      setScanMessage("");
      await loadAll();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  function updateCustomSourceLink(nextLink: string) {
    setScanCandidates([]);
    setScanMessage("");
    const repo = githubRepoFromLink(nextLink);
    setCustomSourceForm((form) => repo
      ? {
        ...form,
        source_kind: "github",
        repo,
        download_url: "",
        page_url: "",
        asset_match: "",
        version: "",
        file_name: "",
        silent_install_args: "",
      }
      : {
        ...form,
        source_kind: "direct",
        repo: "",
        download_url: nextLink,
        page_url: "",
        asset_match: "",
        version: "",
        file_name: "",
        silent_install_args: "",
      });
  }

  async function scanCustomSourceUrl(overrideLink?: string) {
    const sourceLink = (overrideLink ?? (customSourceForm.source_kind === "github"
      ? customSourceForm.repo
      : customSourceForm.download_url)).trim();
    if (!sourceLink) {
      setScanMessage("请先粘贴官网、下载页或安装包链接。");
      return;
    }

    const repo = githubRepoFromLink(sourceLink);
    const scanTarget: CustomSourceScanTarget = repo
      ? { source_kind: "github", source_url: sourceLink, repo }
      : { source_kind: "direct", source_url: normalizeHttpLink(sourceLink), repo: "" };
    setCustomSourceForm((form) => scanTarget.source_kind === "github"
      ? {
        ...form,
        source_kind: "github",
        repo: scanTarget.repo,
        download_url: "",
        page_url: "",
        asset_match: "",
        version: "",
        file_name: "",
        silent_install_args: "",
      }
      : {
        ...form,
        source_kind: "direct",
        repo: "",
        download_url: scanTarget.source_url,
        page_url: "",
        asset_match: "",
        version: "",
        file_name: "",
        silent_install_args: "",
      });

    try {
      setScanBusy(true);
      setScanMessage(scanTarget.source_kind === "github" ? "正在读取 GitHub 最新版本…" : "正在识别官网安装包…");
      const candidates = await invoke<DownloadCandidate[]>("scan_download_candidates", {
        url: scanTarget.source_kind === "github" ? scanTarget.repo : scanTarget.source_url,
      });
      setScanCandidates(candidates);
      if (candidates.length === 0) {
        setScanMessage("没有识别到 Windows 安装包。可以换官网的下载页，或直接粘贴 .exe、.msi、.zip 链接。");
      } else {
        selectDownloadCandidate(candidates[0], scanTarget, true);
      }
    } catch (e) {
      setScanCandidates([]);
      setScanMessage(String(e));
    } finally {
      setScanBusy(false);
    }
  }

  function selectDownloadCandidate(
    candidate: DownloadCandidate,
    target?: CustomSourceScanTarget,
    isAutoSelected = false,
  ) {
    const sourceKind = target?.source_kind ?? customSourceForm.source_kind;
    if (sourceKind === "github") {
      const repo = target?.repo || githubRepoFromLink(customSourceForm.repo) || customSourceForm.repo;
      setCustomSourceForm((form) => ({
        ...form,
        repo: repo || form.repo,
        source_kind: "github",
        display_name: form.display_name.trim() || candidate.display_name || titleFromRepo(repo || form.repo),
        asset_match: candidate.matcher,
        version: "",
        file_name: "",
      }));
      setScanMessage(`${isAutoSelected ? "已识别" : "已选择"} ${candidate.display_name || titleFromRepo(repo)} · ${candidate.version} · ${candidate.size ? formatSize(candidate.size) : "大小未知"}`);
      return;
    }
    const sourceUrl = target?.source_url || customSourceForm.download_url.trim() || candidate.source_page;
    const inferredName = candidate.display_name || titleFromFileName(candidate.file_name);
    setCustomSourceForm((form) => ({
      ...form,
      source_kind: "direct",
      repo: "",
      display_name: form.display_name.trim() || inferredName,
      download_url: sourceUrl,
      page_url: sourceUrl,
      asset_match: candidate.matcher,
      version: "",
      file_name: "",
    }));
    setScanMessage(`${isAutoSelected ? "已识别" : "已选择"} ${inferredName || candidate.file_name} · ${candidate.version} · ${candidate.size ? formatSize(candidate.size) : "大小未知"}`);
  }

  async function removeCustomSource(sw: SoftwareInfo) {
    if (!isUserSource(sw)) return;
    const ok = window.confirm(`确定删除下载源「${sw.display_name}」？\n\n已缓存的安装包不会自动删除。`);
    if (!ok) return;
    try {
      setLoading(true);
      await invoke("remove_custom_software", { id: sw.id });
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(sw.id);
        return next;
      });
      if (libraryDetailId === sw.id) setLibraryDetailId(null);
      await loadAll();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function refreshCustomSourceIcon(sw: SoftwareInfo, mode: "auto" | "clipboard" | "clear") {
    if (!isUserSource(sw)) return;
    try {
      setLoading(true);
      if (mode === "auto") {
        await invoke<string>("fetch_custom_software_icon", { id: sw.id });
      } else if (mode === "clipboard") {
        await invoke<string>("save_custom_software_icon_from_clipboard", { id: sw.id });
      } else {
        await invoke("clear_custom_software_icon", { id: sw.id });
      }
      await loadAll();
      if (libraryDetailId) setLibraryDetailId(sw.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function uninstallOne(sw: SoftwareInfo) {
    const id = sw.id;
    setStatus((prev) => ({ ...prev, [id]: { state: "uninstalling", percent: 0, message: "卸载中…" } }));
    try {
      await invoke<{ message: string }>("uninstall_software", { id });
      await detectInstalledNow([sw]);
    } catch (e) {
      setStatus((prev) => ({ ...prev, [id]: { state: "uninstall_failed", percent: 0, message: String(e) } }));
    }
  }

  async function batchInstall() {
    const items = selectedInstallableItems;
    if (!items.length) return;
    setBatchBusy(true);
    await Promise.all(items.map((sw) => installOne(sw)));
    setBatchBusy(false);
  }

  async function batchUninstall() {
    if (!uninstallableItems.length) return;
    const ok = window.confirm(
      `确定卸载 ${uninstallableItems.length} 个软件？\n\n将删除安装目录和桌面快捷方式\n${uninstallableItems.map((sw) => `· ${sw.display_name}`).join("\n")}`
    );
    if (!ok) return;
    setBatchBusy(true);
    await Promise.all(uninstallableItems.map((sw) => uninstallOne(sw)));
    setBatchBusy(false);
  }

  function toggleRow(id: string, canSelect: boolean) {
    if (!canSelect || batchBusy) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  function rowStatus(sw: SoftwareInfo): { label: string; kind: string } {
    const st = status[sw.id];
    if (st?.state === "downloading") return { label: st.message, kind: "busy" };
    if (st?.state === "paused") return { label: st.message, kind: "busy" };
    if (st?.state === "cancelling") return { label: "正在取消", kind: "busy" };
    if (st?.state === "cancelled") return { label: "已取消", kind: "idle" };
    if (st?.state === "installing") return { label: st.message, kind: "busy" };
    if (st?.state === "uninstalling") return { label: st.message, kind: "busy" };
    if (st?.state === "checking") return { label: st.message, kind: "busy" };
    if (st?.state === "uninstall_failed") return { label: "卸载失败", kind: "uninstall-failed" };
    if (st?.state === "error") return { label: st.message || "安装失败", kind: "error" };
    if (st?.state === "done") return { label: st.message, kind: "ok" };
    if (isMicrosoftStoreApp(sw)) {
      return installed[sw.id]
        ? { label: "已安装", kind: "installed" }
        : { label: "商店可安装", kind: "idle" };
    }
    if (sw.install_kind === "download") {
      if (!sw.portable) return { label: sw.latest_version.startsWith("查询失败") ? "检测失败" : "无下载项", kind: "warn" };
      if (packageCache[sw.id]?.cached) return { label: "已缓存", kind: "cached" };
      return { label: "未下载", kind: "idle" };
    }
    if (sw.install_kind === "installer") {
      if (installed[sw.id]) return { label: "已安装", kind: "installed" };
      if (packageCache[sw.id]?.cached) return { label: "已缓存", kind: "cached" };
      return { label: "未下载", kind: "idle" };
    }
    if (installed[sw.id]) return { label: "已安装", kind: "installed" };
    if (!sw.portable) return { label: sw.latest_version.startsWith("查询失败") ? "检测失败" : "无便携版", kind: "warn" };
    if (packageCache[sw.id]?.cached) return { label: "已缓存", kind: "cached" };
    return { label: "未安装", kind: "idle" };
  }

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "F4" && e.altKey) {
        e.preventDefault();
        void getCurrentWindow().close().catch(() => getCurrentWindow().destroy()).catch(() => invoke("exit_app"));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  /*  NAV items  */
  const NAV: { id: NavId; label: string; icon: NavId; badge?: string }[] = [
    { id: "library", label: "软件库", icon: "library" },
    { id: "packages", label: "安装包", icon: "packages" },
    { id: "winget", label: "Winget", icon: "winget" },
    { id: "automation", label: "自动化", icon: "automation" },
    { id: "settings", label: "设置", icon: "settings" },
  ];

  /*  page renderers  */

  function renderDownloadControls(sw: SoftwareInfo) {
    const state = status[sw.id]?.state;
    if (state !== "downloading" && state !== "paused" && state !== "cancelling") return null;

    if (state === "cancelling") {
      return (
        <button className="btn btn-sm" type="button" disabled>
          取消中
        </button>
      );
    }

    return (
      <>
        <button className="btn btn-sm btn-primary" type="button" onClick={() => void (state === "paused" ? resumeDownload(sw) : pauseDownload(sw))}>
          {state === "paused" ? "继续" : "暂停"}
        </button>
        <button className="btn btn-sm btn-danger" type="button" onClick={() => void cancelDownload(sw)}>
          取消
        </button>
      </>
    );
  }

  function renderLibraryPrimaryActions(sw: SoftwareInfo, mode: "list" | "detail") {
    const isOfficial = softwareSourceKind(sw) === "official";
    const isStore = isMicrosoftStoreApp(sw);
    const isBusy = isItemBusy(sw.id);
    const isInstalled = installed[sw.id];
    const canInstallGithub = sw.install_kind === "portable" && !!sw.portable;
    const downloadOnly = isDownloadOnly(sw);
    const cached = packageCache[sw.id]?.cached;
    const canSilentInstall = canRunSilentInstall(sw);
    

    const detailBtn = (
      <button
        className={`btn btn-sm btn-ghost library-action-detail${mode === "detail" ? " is-current" : ""}`}
        type="button"
        disabled={mode === "detail"}
        onClick={mode === "list" ? () => openLibraryDetail(sw.id) : undefined}
      >
        详情
      </button>
    );

    if (isDownloadControllable(sw.id)) {
      return mode === "list" ? <>{renderDownloadControls(sw)}</> : null;
    }

    let installBtn;
    if (isStore) {
      installBtn = (
        <button
          className="btn btn-sm btn-primary library-action-install"
          type="button"
          disabled={isBusy}
          onClick={() => void openMicrosoftStore(sw)}
        >
          打开商店
        </button>
      );
    } else if (downloadOnly) {
      installBtn = (
        <button
          className="btn btn-sm btn-primary library-action-install"
          type="button"
          disabled={isBusy || !sw.portable}
          onClick={() => void (cached ? (canSilentInstall ? runSilentInstaller(sw) : openCachedPackage(sw)) : cacheOne(sw))}
        >
          {cached ? (canSilentInstall ? "安装" : "打开") : "下载"}
        </button>
      );
    } else if (isInstalled) {
      installBtn = (
        <button
          className="btn btn-sm btn-danger library-action-install"
          type="button"
          disabled={isBusy || batchBusy}
          onClick={() => void (isOfficial ? uninstallInstaller(sw) : uninstallOne(sw))}
        >
          卸载
        </button>
      );
    } else if (isOfficial) {
      installBtn = (
        <button
          className="btn btn-sm btn-primary library-action-install"
          type="button"
          disabled={isBusy}
          onClick={() => void installOfficialSoftware(sw)}
        >
          安装
        </button>
      );
    } else if (canInstallGithub) {
      installBtn = (
        <button
          className="btn btn-sm btn-primary library-action-install"
          type="button"
          disabled={isBusy || batchBusy}
          onClick={() => void installOne(sw)}
        >
          安装
        </button>
      );
    } else {
      installBtn = (
        <button className="btn btn-sm btn-primary library-action-install" type="button" disabled>
          安装
        </button>
      );
    }

    return (
      <>
        {detailBtn}
        {installBtn}
      </>
    );
  }

  function renderLibraryItemColumns(sw: SoftwareInfo) {
    const { label, kind } = rowStatus(sw);
    const icon = softwareIconSrc(sw);

    return (
      <>
        <div className="col-name">
          <span className={`row-avatar ${icon ? "custom-icon-mark" : "brand-mark"}`} aria-hidden="true">
            {icon ? <img src={icon} alt="" /> : <span className="brand-mark-core" />}
          </span>
          <div className="row-text">
            <span className="row-title" title={sw.display_name}>{sw.display_name}</span>
            <span className="row-sub" title={sw.portable?.name ?? softwareKindLabel(sw)}>
              {softwareKindLabel(sw)}{sw.portable?.name ? ` · ${sw.portable.name}` : ""}
            </span>
          </div>
        </div>
        <span className={`col-source source-chip source-${softwareSourceKind(sw)}`}>{softwareSourceLabel(sw)}</span>
        <span className="col-ver">
          <span className="ver-chip">{versionDisplay(sw)}</span>
        </span>
        <span className="col-date">{formatDate(sw.published_at)}</span>
        <span className="col-size">{sw.portable ? formatSize(sw.portable.size) : ""}</span>
        <span className={`col-status status-chip status-${kind}`}>{label}</span>
      </>
    );
  }

  function renderLibrarySoftwareDetail(sw: SoftwareInfo) {
    const theme = APP_THEME[sw.id] || "theme-mint";
    const isOfficial = softwareSourceKind(sw) === "official";
    const isStore = isMicrosoftStoreApp(sw);
    const storeProductId = microsoftStoreProductId(sw);
    const downloadOnly = isDownloadOnly(sw);
    const userSource = isUserSource(sw);
    const cached = packageCache[sw.id]?.cached;
    const cacheInfo = packageCache[sw.id];
    const st = status[sw.id];
    const isBusy = isItemBusy(sw.id);
    const installDir = `${installBasePath}\\${sw.id}`;
    const paths = appPaths[sw.id];

    return (
      <div className={`page page-stack library-detail-page ${theme}`}>
        <button className="btn btn-sm btn-ghost library-detail-back" type="button" onClick={closeLibraryDetail}>
          返回列表
        </button>

        <div className="library-detail-table">
          <div className="list-header list-header-library">
            <span className="col-check" />
            <span className="col-name">软件</span>
            <span className="col-source">来源</span>
            <span className="col-ver">版本/渠道</span>
            <span className="col-date">发布</span>
            <span className="col-size">大小</span>
            <span className="col-status">状态</span>
            <span className="col-actions">操作</span>
          </div>
          <div className={`list-row list-row-library library-detail-summary-row ${theme}${installed[sw.id] ? " row-installed" : ""}${isBusy ? " row-busy" : ""}`}>
            <span className="col-check" aria-hidden="true" />
            {renderLibraryItemColumns(sw)}
            <div className="col-actions">{renderLibraryPrimaryActions(sw, "detail")}</div>
          </div>
        </div>

        {isBusy && st && (
          <div className="pkg-progress">
            <div className="pkg-progress-bar" style={{ width: `${st.percent}%` }} />
            <span className="pkg-progress-text">{st.message}</span>
          </div>
        )}

        <section className="library-detail-panel">
          <h3>详细信息</h3>
          {isStore ? (
            <div className="library-detail-grid">
              <div>
                <span>安装方式</span>
                <code>Microsoft Store</code>
              </div>
              <div>
                <span>应用包</span>
                <code>OpenAI.Codex</code>
              </div>
              <div>
                <span>商店编号</span>
                <code>{storeProductId || "—"}</code>
              </div>
              <div>
                <span>更新方式</span>
                <code>由 Microsoft Store 管理更新</code>
              </div>
              <div>
                <span>商店网页</span>
                <code>{sw.release_url || "—"}</code>
              </div>
            </div>
          ) : (
            <div className="library-detail-grid">
              <div>
                <span>安装包</span>
                <code>{sw.portable?.name || "—"}</code>
              </div>
              <div>
                <span>来源</span>
                <code>{softwareSourceLabel(sw)} · {softwareKindLabel(sw)}</code>
              </div>
              <div>
                <span>{isOfficial ? "官网" : "发布页"}</span>
                <code>{sw.release_url || "—"}</code>
              </div>
              <div>
                <span>{downloadOnly ? "下载缓存" : "安装目录"}</span>
                <code>{paths?.install_dir || installDir}</code>
              </div>
              <div>
                <span>缓存路径</span>
                <code>{cacheInfo?.path || (cached ? "已缓存" : "未缓存")}</code>
              </div>
              {isOfficial && !downloadOnly && (
                <div>
                  <span>安装流程</span>
                  <code>官网源 · 缓存 → 检测安装 → 自动化/打开安装器</code>
                </div>
              )}
              {downloadOnly && (
                <div>
                  <span>处理方式</span>
                  <code>{canRunSilentInstall(sw) ? "下载到本地后，按保存的参数静默安装" : "只下载到 data\\packages，不执行安装"}</code>
                </div>
              )}
            </div>
          )}
        </section>

        {isStore ? (
          <div className="library-detail-actions">
            <button className="btn btn-sm btn-primary" type="button" disabled={isBusy} onClick={() => void openMicrosoftStore(sw)}>
              打开 Microsoft Store
            </button>
            <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void detectInstalledNow([sw])}>
              检测安装
            </button>
            {sw.release_url && (
              <button className="btn btn-sm btn-ghost" type="button" onClick={() => void openUrl(sw.release_url)}>
                打开商店网页
              </button>
            )}
          </div>
        ) : downloadOnly ? (
          <div className="library-detail-actions">
            {renderDownloadControls(sw)}
            {canRunSilentInstall(sw) && (
              <button className="btn btn-sm btn-primary" type="button" disabled={!cached || isBusy} onClick={() => void runSilentInstaller(sw)}>
                静默安装
              </button>
            )}
            <button className="btn btn-sm btn-primary" type="button" disabled={isBusy || !sw.portable} onClick={() => void cacheOne(sw)}>
              {cached ? "刷新缓存" : "下载"}
            </button>
            <button className="btn btn-sm" type="button" disabled={!cached || isBusy} onClick={() => void openCachedPackage(sw)}>
              打开缓存
            </button>
            {sw.release_url && (
              <button className="btn btn-sm btn-ghost" type="button" onClick={() => void openUrl(sw.release_url)}>
                打开来源
              </button>
            )}
            {userSource && (
              <button className="btn btn-sm" type="button" disabled={isBusy || loading} onClick={() => void openEditCustomSourceModal(sw)}>
                编辑源
              </button>
            )}
            {userSource && (
              <button className="btn btn-sm" type="button" disabled={isBusy || loading} onClick={() => void refreshCustomSourceIcon(sw, "auto")}>
                自动获取图标
              </button>
            )}
            {userSource && (
              <button className="btn btn-sm" type="button" disabled={isBusy || loading} onClick={() => void refreshCustomSourceIcon(sw, "clipboard")}>
                保存截图图标
              </button>
            )}
            {userSource && sw.icon_path && (
              <button className="btn btn-sm btn-ghost" type="button" disabled={isBusy || loading} onClick={() => void refreshCustomSourceIcon(sw, "clear")}>
                恢复默认图标
              </button>
            )}
            {userSource && (
              <button className="btn btn-sm btn-danger" type="button" disabled={isBusy || loading} onClick={() => void removeCustomSource(sw)}>
                删除源
              </button>
            )}
          </div>
        ) : isOfficial ? (
          <div className="library-detail-actions">
            {renderDownloadControls(sw)}
            <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void detectInstalledNow([sw])}>
              检测安装
            </button>
            <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void cacheOne(sw)}>
              {cached ? "刷新缓存" : "下载并缓存"}
            </button>
            <button className="btn btn-sm" type="button" disabled={!cached || isBusy} onClick={() => void openCachedPackage(sw)}>
              打开安装包
            </button>
            <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void copyInstallDir(sw)}>
              复制安装路径
            </button>
            <button className="btn btn-sm btn-ghost" type="button" onClick={() => setNav("automation")}>
              编辑步骤
            </button>
            {sw.release_url && (
              <button className="btn btn-sm btn-ghost" type="button" onClick={() => void openUrl(sw.release_url)}>
                打开官网
              </button>
            )}
          </div>
        ) : (
          (isDownloadControllable(sw.id) || sw.release_url) && (
            <div className="library-detail-actions">
              {renderDownloadControls(sw)}
              {sw.release_url && (
                <button className="btn btn-sm btn-ghost" type="button" onClick={() => void openUrl(sw.release_url)}>
                  打开 GitHub Release
                </button>
              )}
            </div>
          )
        )}

        {st && (st.state === "error" || st.state === "uninstall_failed" || st.state === "done") && (
          <p className={`pkg-hint pkg-hint-${st.state}`}>{st.message}</p>
        )}
      </div>
    );
  }

  function renderLibraryPage() {
    const detailSw = libraryDetailId ? software.find((s) => s.id === libraryDetailId) ?? null : null;
    if (detailSw) return renderLibrarySoftwareDetail(detailSw);

    const libSelectable = portableItems.filter((sw) => sw.portable);
    return (
      <div className="page page-stack">
        <header className="page-head">
          <div>
            <p className="page-kicker">便携版 · 官网安装包 · Microsoft Store · 自定义下载源</p>
            <h2>软件库</h2>
            <p className="page-sub">
              {libraryAvailableItems.length} 个可用（GitHub {githubLibraryItems.length} · 官网 {officialLibraryItems.length} · 商店 {storeLibraryItems.length} · 下载源 {downloadOnlyItems.length}）· {notInstalledCount} 个未安装 · {loading ? "同步中" : software.length ? "检测完成" : "未检测"}
            </p>
          </div>
          <button className="btn btn-ghost btn-refresh" type="button" onClick={loadAll} disabled={loading || batchBusy} title="刷新">
            <span className={`refresh-icon${loading ? " is-spinning" : ""}`} aria-hidden="true"></span>
          </button>
        </header>

        <div className="insight-grid">
          <div className="insight-card">
            <span>源健康</span>
            <strong>{sourceHealth}</strong>
          </div>
          <div className="insight-card">
            <span>GitHub 便携版</span>
            <strong>{githubLibraryItems.length} 个</strong>
          </div>
          <div className="insight-card">
            <span>官网安装包</span>
            <strong>{officialLibraryItems.length} 个</strong>
          </div>
          <div className="insight-card">
            <span>下载源</span>
            <strong>{downloadOnlyItems.length} 个</strong>
          </div>
        </div>

        <div className="library-filter-bar" role="tablist" aria-label="按来源筛选">
          <button
            type="button"
            className={`library-filter-chip${librarySourceFilter === "all" ? " is-active" : ""}`}
            onClick={() => setLibrarySourceFilter("all")}
          >
            全部 {selectable.length}
          </button>
          <button
            type="button"
            className={`library-filter-chip library-filter-github${librarySourceFilter === "github" ? " is-active" : ""}`}
            onClick={() => setLibrarySourceFilter("github")}
          >
            GitHub {githubLibraryItems.length}
          </button>
          <button
            type="button"
            className={`library-filter-chip library-filter-official${librarySourceFilter === "official" ? " is-active" : ""}`}
            onClick={() => setLibrarySourceFilter("official")}
          >
            官网 {officialLibraryItems.length}
          </button>
          <button
            type="button"
            className={`library-filter-chip library-filter-store${librarySourceFilter === "store" ? " is-active" : ""}`}
            onClick={() => setLibrarySourceFilter("store")}
          >
            商店 {storeLibraryItems.length}
          </button>
        </div>

        <p className="library-open-hint">官网、商店和下载源单击行进入详情；GitHub 便携版双击行进入详情，单击勾选后点「安装」。</p>

        <div className="action-bar">
          <span className={`sel-count${selected.size ? " sel-count-on" : ""}`}>
            {selected.size ? `已选 ${selected.size}` : "未选择"}
          </span>
          <button className="btn btn-link" type="button" onClick={() => setSelected(new Set(libSelectable.map((s) => s.id)))} disabled={batchBusy || !libSelectable.length}>全选 GitHub</button>
          <button className="btn btn-link" type="button" onClick={() => setSelected(new Set())} disabled={batchBusy || !selected.size}>清空</button>
          {error && <span className="action-error">{error}</span>}
          <div className="action-spacer" />
          <button className="btn" type="button" onClick={openNewCustomSourceModal} disabled={loading || batchBusy}>添加软件链接</button>
          <button className="btn" type="button" onClick={() => void detectInstalledNow()} disabled={loading || batchBusy || !libraryAvailableItems.length}>一键检测</button>
          <button className="btn btn-primary" type="button" onClick={batchInstall} disabled={batchBusy || !installableIds.length}>
            {primaryActionLabel}{installableIds.length ? ` (${installableIds.length})` : ""}
          </button>
          {uninstallableItems.length > 0 && (
            <button className="btn btn-danger" type="button" onClick={batchUninstall} disabled={batchBusy}>
              卸载 ({uninstallableItems.length})
            </button>
          )}
        </div>

        <div className="list-header list-header-library">
          <span className="col-check" />
          <span className="col-name">软件</span>
          <span className="col-source">来源</span>
          <span className="col-ver">版本/渠道</span>
          <span className="col-date">发布</span>
          <span className="col-size">大小</span>
          <span className="col-status">状态</span>
          <span className="col-actions">操作</span>
        </div>

        <div className="list-body">
          {libraryItems.map((sw) => {
            const checked = selected.has(sw.id);
            const isGithubPortable = sw.install_kind === "portable" && softwareSourceKind(sw) === "github";
            const canSelect = isGithubPortable && !!sw.portable && !batchBusy;
            const theme = APP_THEME[sw.id] || "theme-mint";
            const st = status[sw.id];
            const isBusy = isItemBusy(sw.id);
            const isOfficial = softwareSourceKind(sw) === "official";
            const isStore = isMicrosoftStoreApp(sw);

            return (
              <div
                key={sw.id}
                className={`list-row list-row-library ${theme}${checked ? " row-selected" : ""}${installed[sw.id] ? " row-installed" : ""}${isBusy ? " row-busy" : ""}${isStore ? " row-store row-openable" : isOfficial ? " row-official row-openable" : " row-github row-openable"}`}
                style={st?.state === "downloading" ? { ["--dl-progress" as string]: `${st.percent}%` } : undefined}
                onClick={() => {
                  if (isOfficial || isStore || isDownloadOnly(sw)) openLibraryDetail(sw.id);
                  else toggleRow(sw.id, canSelect);
                }}
                onDoubleClick={() => openLibraryDetail(sw.id)}
                title={isOfficial ? "" : ""}
              >
                <label className="col-check" onClick={(e) => e.stopPropagation()}>
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={!canSelect}
                    onChange={(e) => {
                      setSelected((prev) => {
                        const next = new Set(prev);
                        if (e.target.checked) next.add(sw.id); else next.delete(sw.id);
                        return next;
                      });
                    }}
                  />
                </label>
                {renderLibraryItemColumns(sw)}
                <div className="col-actions" onClick={(e) => e.stopPropagation()}>
                  {renderLibraryPrimaryActions(sw, "list")}
                </div>
              </div>
            );
          })}
          {libraryItems.length === 0 && !loading && (
            <div className="source-empty">
              <strong>
                {librarySourceFilter === "official"
                  ? "暂无官网安装包"
                  : librarySourceFilter === "store"
                    ? "暂无 Microsoft Store 应用"
                  : librarySourceFilter === "github"
                    ? "暂无 GitHub 便携版"
                    : failedItems.length
                      ? "软件源暂时不可用"
                      : "暂无软件"}
              </strong>
              <span>
                {error ||
                  failedItems.map((sw) => `${sw.display_name}: ${sw.latest_version}`).join("； ") ||
                  "请切换筛选或重新检测。"}
              </span>
              <div className="source-empty-actions">
                <button className="btn btn-sm btn-primary" type="button" onClick={loadAll} disabled={loading}>
                  刷新
                </button>
                {librarySourceFilter !== "all" && (
                  <button className="btn btn-sm" type="button" onClick={() => setLibrarySourceFilter("all")}>
                    查看全部
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  async function waitForInstalledState(id: string, timeoutMs = 30000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() <= deadline) {
      const ok = await invoke<boolean>("is_software_installed", { id });
      if (ok) return true;
      await sleep(1500);
    }
    return false;
  }

  async function runInstallerAutomationFlow(sw: SoftwareInfo) {
    if (!sw.portable) return;
    const id = sw.id;
    const invokeArgs = {
      id,
      version: sw.latest_version,
      fileName: sw.portable.name,
    };

    setStatus((prev) => ({ ...prev, [id]: { state: "installing", percent: 45, message: "正在打开官方安装器" } }));
    const launch = await invoke<WeGameInstallResult>("launch_wegame_installer_cmd", invokeArgs);
    if (!launch.success) throw new Error(launch.message || "启动失败");
    setStatus((prev) => ({ ...prev, [id]: { state: "installing", percent: 55, message: `${launch.message} · 等待安装窗口` } }));
    await sleep(3500);

    const steps = await loadRunnableAutomationSteps();
    if (!steps.length) throw new Error("没有可执行的自动化步骤，请先在自动化页配置 WeGame 步骤");

    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      const progress = Math.min(92, 60 + Math.round((i / Math.max(steps.length, 1)) * 30));
      if (!step.timeAfterStep && step.delayMs > 0) {
        setStatus((prev) => ({
          ...prev,
          [id]: {
            state: "installing",
            percent: progress,
            message: `等待 ${formatDelay(step.delayMs, delayUnitOf(step))} 后执行「${step.name}」`,
          },
        }));
        await sleep(step.delayMs);
      }

      setStatus((prev) => ({
        ...prev,
        [id]: {
          state: "installing",
          percent: progress,
          message: `执行 ${i + 1}/${steps.length}: ${step.name}`,
        },
      }));
      const result = await invoke<VisualTargetResult>("run_automation_step_cmd", { stepId: step.id, dryRun: false });
      if (!result.success) throw new Error(`步骤「${step.name}」失败: ${result.message}`);

      if (step.timeAfterStep && step.delayMs > 0) {
        setStatus((prev) => ({
          ...prev,
          [id]: {
            state: "installing",
            percent: Math.min(94, progress + 2),
            message: `步骤「${step.name}」完成，等待 ${formatDelay(step.delayMs, delayUnitOf(step))}`,
          },
        }));
        await sleep(step.delayMs);
      }
    }

    setStatus((prev) => ({
      ...prev,
      [id]: {
        state: "installing",
        percent: 95,
        message: "自动化已执行，正在检测安装结果",
      },
    }));
  }

  async function installOfficialSoftware(sw: SoftwareInfo) {
    if (!sw.portable || sw.install_kind !== "installer" || isItemBusy(sw.id)) return;
    const id = sw.id;

    setStatus((prev) => ({ ...prev, [id]: { state: "installing", percent: 0, message: "准备安装" } }));

    try {
      if (!packageCache[id]?.cached) {
        setStatus((prev) => ({ ...prev, [id]: { state: "downloading", percent: 0, message: "下载安装包" } }));
        if (!(await cacheOne(sw))) return;
        await refreshPackageCache([sw], false);
      }

      if (!sw.ocr_install) {
        await openFreshCachedPackage(sw);
        return;
      }

      await runInstallerAutomationFlow(sw);
      const detected = await waitForInstalledState(id);
      if (!detected) {
        throw new Error("自动化已执行，但暂未检测到 WeGame 安装结果。可以稍后点刷新，或检查自动化最后一步是否过早关闭窗口。");
      }
      await detectInstalledNow([sw]);
    } catch (e) {
      setStatus((prev) => ({ ...prev, [id]: { state: "error", percent: 0, message: String(e) } }));
    }
  }

  function renderPackagesPage() {
    const pkgItems = installerItems;

    return (
      <div className="page page-stack">
        <header className="page-head">
          <div>
            <p className="page-kicker">官网安装包 · 自动下载与安装</p>
            <h2>安装包</h2><p className="page-sub">{pkgItems.length} 个源 · {pkgItems.filter((sw) => packageCache[sw.id]?.cached).length} </p>
          </div>
          <button className="btn btn-ghost btn-refresh" type="button" onClick={loadAll} disabled={loading} title="刷新">
            <span className={`refresh-icon${loading ? " is-spinning" : ""}`}></span>
          </button>
        </header>

        <section className="package-command-center">
          <div>
            <span className="command-kicker">官方安装包策略</span><strong>无缓存自动下载；WeGame 执行自动化，AMD 打开官方安装器。</strong>
          </div>
          <div className="command-metrics">
            <span>{pkgItems.length} 个源</span>
            <span>{pkgItems.filter((sw) => packageCache[sw.id]?.cached).length} 个缓存</span>
            <span>{formatSize(cachedBytes)} 本地包</span>
          </div>
          <button className="btn btn-sm" type="button" onClick={() => void detectInstalledNow(pkgItems)} disabled={loading || !pkgItems.length}>
            重新检测
          </button>
        </section>

        {pkgItems.length === 0 && <p className="auto-empty">暂无安装包源。</p>}
        {pkgItems.map((sw) => {
          const cached = packageCache[sw.id]?.cached;
          const cacheInfo = packageCache[sw.id];
          const st = status[sw.id];
          const isBusy = isItemBusy(sw.id);
          const isInstalled = installed[sw.id];
          const installStatusKind =
            st?.state === "uninstall_failed"
              ? "uninstall-failed"
              : st?.state === "error"
                ? "error"
                : st?.state === "uninstalling" || st?.state === "paused" || st?.state === "cancelling"
                  ? "busy"
                  : isInstalled
                    ? "installed"
                    : "idle";
          const installStatusText =
            st?.state === "uninstall_failed"
              ? "卸载失败"
              : st?.state === "error"
                ? st.message || "安装失败"
                : st?.state === "paused"
                  ? "已暂停"
                  : st?.state === "cancelling"
                    ? "正在取消…"
                    : st?.state === "cancelled"
                      ? "已取消"
                      : st?.state === "uninstalling"
                  ? "卸载中…"
                  : isInstalled
                    ? "已安装"
                    : cached
                      ? "已缓存"
                      : "未下载";
          const installDir = `${installBasePath}\\${sw.id}`;
          const icon = softwareIconSrc(sw);
          return (
            <div key={sw.id} className="pkg-card">
              <div className="pkg-card-top">
                <div className="pkg-title">
                  <span className={`pkg-avatar ${icon ? "custom-icon-mark" : "brand-mark"}`} aria-hidden="true">
                    {icon ? <img src={icon} alt="" /> : <span className="brand-mark-core" />}
                  </span>
                  <div>
                    <strong>{sw.display_name}</strong>
                    <span className="pkg-meta">{sw.latest_version} · {formatSize(sw.portable!.size)} 安装包</span>
                  </div>
                </div>
                <span className={`status-chip status-${installStatusKind}`}>
                  {installStatusText}
                </span>
              </div>

              <div className="pkg-flow pkg-flow-3" aria-label={`${sw.display_name} 安装工作流`}>
                <span className="pkg-flow-step done"></span>
                <span className={`pkg-flow-step${cached ? " done" : " active"}`}></span>
                <span className={`pkg-flow-step${isInstalled ? " done" : cached ? " active" : ""}`}></span>
              </div>

              <div className="pkg-path-grid">
                <div>
                  <span>缓存文件</span>
                  <code>{cacheInfo?.path || sw.portable!.name}</code>
                </div>
                <div>
                  <span>目标目录</span>
                  <code>{installDir}</code>
                </div>
              </div>

              {isBusy && st && (
                <div className="pkg-progress">
                  <div className="pkg-progress-bar" style={{ width: `${st.percent}%` }} />
                  <span className="pkg-progress-text">{st.message}</span>
                </div>
              )}

              <div className="pkg-card-actions">
                {renderDownloadControls(sw)}
                <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void detectInstalledNow([sw])}>
                  检测安装
                </button>
                {!isInstalled && (
                  <button className="btn btn-sm btn-primary" type="button" disabled={isBusy} onClick={() => void installOfficialSoftware(sw)}>
                    立即安装
                  </button>
                )}
                <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void cacheOne(sw)}>
                  {cached ? "刷新缓存" : "下载并缓存"}
                </button>
                {isInstalled && (
                  <button className="btn btn-sm btn-danger" type="button" disabled={isBusy} onClick={() => void uninstallInstaller(sw)}>
                    卸载
                  </button>
                )}
                <button className="btn btn-sm" type="button" disabled={!cached || isBusy} onClick={() => void openCachedPackage(sw)}>
                  打开安装包
                </button>
                <button className="btn btn-sm" type="button" disabled={isBusy} onClick={() => void copyInstallDir(sw)}>
                  复制安装路径
                </button>
                <button className="btn btn-sm btn-ghost" type="button" onClick={() => setNav("automation")}>
                  编辑步骤
                </button>
              </div>
              {st && (st.state === "error" || st.state === "uninstall_failed" || st.state === "done") && (
                <p className={`pkg-hint pkg-hint-${st.state}`}>{st.message}</p>
              )}
            </div>
          );
        })}
      </div>
    );
  }

  function renderSettingsPage() {
    return (
      <div className="page page-stack">
        <header className="page-head">
          <div>
            <p className="page-kicker">低频全局配置</p>
            <h2>设置</h2>
          </div>
        </header>
        <div className="insight-grid">
          <div className="insight-card">
            <span>安装位置</span>
            <strong>{isCustomPath ? "自定义" : "默认"}</strong>
          </div>
          <div className="insight-card">
            <span>缓存包</span>
            <strong>{cachedItems.length} 个</strong>
          </div>
          <div className="insight-card">
            <span>缓存体积</span>
            <strong>{formatSize(cachedBytes)}</strong>
          </div>
          <div className="insight-card">
            <span>已安装</span>
            <strong>{Object.values(installed).filter(Boolean).length} 个</strong>
          </div>
        </div>
        <section className="auto-panel">
          <h3>安装根目录</h3>
          <div className="path-row">
            <input
              id="baseDirInput"
              className={`path-input${isCustomPath ? " path-input-custom" : ""}`}
              value={pathInput}
              onChange={(e) => setPathInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && savePath()}
              spellCheck={false}
            />
          </div>
          <div className="path-actions">
            <button className="btn btn-sm" type="button" onClick={pickFolder}>浏览</button>
            <button className="btn btn-sm btn-primary" type="button" onClick={savePath}>保存</button>
            <button className="btn btn-sm" type="button" onClick={useDefaultPath}>使用默认</button>
            <button className="btn btn-sm" type="button" onClick={resetDefaultPath}>重置</button>
          </div>
          <p className="page-hint">配置在 data\config.json；安装包缓存保存在 data\packages；软件默认装到 AppData\Local\software-manager\apps</p>
        </section>
        <section className="auto-panel settings-dir-panel">
          <h3>目录浏览</h3>
          <DirExplorer
            base={pathSettings?.current_base ?? pathInput}
            software={software}
            selectedId={selectedOneId}
            appPaths={appPaths}
            installed={installed}
            packageCache={packageCache}
          />
        </section>
      </div>
    );
  }

  function renderCustomSourceModal() {
    if (!showCustomSource) return null;

    const isGithub = customSourceForm.source_kind === "github";
    const sourceLink = isGithub ? customSourceForm.repo : customSourceForm.download_url;
    const selectedCandidate = scanCandidates.find((candidate) => candidate.matcher === customSourceForm.asset_match)
      ?? scanCandidates[0];
    const recognizedName = selectedCandidate
      ? customSourceForm.display_name || selectedCandidate.display_name || titleFromFileName(selectedCandidate.file_name)
      : "";
    const canSave = Boolean(editingCustomSourceId || scanCandidates.length > 0);
    const closeCustomSourceModal = () => {
      setShowCustomSource(false);
      setEditingCustomSourceId(null);
      setScanCandidates([]);
      setScanMessage("");
    };
    return (
      <div className="custom-software-overlay" onMouseDown={closeCustomSourceModal}>
        <form className="custom-software-modal" onSubmit={saveCustomSource} onMouseDown={(e) => e.stopPropagation()}>
          <h3>{editingCustomSourceId ? "编辑软件链接" : "添加软件链接"}</h3>
          <p className="custom-source-intro">粘贴官网、下载页或安装包链接。系统会识别软件、版本和 Windows 安装包。</p>
          <div className="form-group">
            <label>软件链接</label>
            <div className="source-scan-row">
              <input
                required
                value={sourceLink}
                onChange={(event) => updateCustomSourceLink(event.target.value)}
                onPaste={(event) => {
                  const pasted = event.clipboardData.getData("text").trim();
                  if (pasted) window.setTimeout(() => void scanCustomSourceUrl(pasted), 0);
                }}
                placeholder="粘贴官网、下载页、GitHub 或安装包链接"
                spellCheck={false}
              />
              <button className="btn btn-sm" type="button" onClick={() => void scanCustomSourceUrl()} disabled={scanBusy || !sourceLink.trim()}>
                {scanBusy ? "识别中" : "识别链接"}
              </button>
            </div>
          </div>

          {(scanMessage || selectedCandidate) && (
            <section className="link-recognition" aria-live="polite">
              {scanMessage && <p className="scan-message">{scanMessage}</p>}
              {selectedCandidate && (
                <div className="recognized-package">
                  <span>已识别</span>
                  <strong>{recognizedName || selectedCandidate.file_name}</strong>
                  <p>{selectedCandidate.version} · {selectedCandidate.size ? formatSize(selectedCandidate.size) : "大小未知"}</p>
                  <small>{selectedCandidate.file_name}</small>
                </div>
              )}
              {scanCandidates.length > 1 && (
                <div className="scan-candidates">
                  {scanCandidates.slice(0, 5).map((candidate) => (
                    <button
                      key={candidate.url}
                      className={`scan-candidate${candidate.url === selectedCandidate?.url ? " is-selected" : ""}`}
                      type="button"
                      onClick={() => selectDownloadCandidate(candidate)}
                      title={candidate.file_name}
                    >
                      <span className="scan-candidate-main">
                        <strong>{candidate.display_name || titleFromFileName(candidate.file_name) || candidate.file_name}</strong>
                        <span>{candidate.version} · {candidate.size ? formatSize(candidate.size) : "大小未知"}</span>
                      </span>
                      <span className="scan-candidate-rule">选用</span>
                    </button>
                  ))}
                </div>
              )}
            </section>
          )}

          <details className="custom-source-advanced">
            <summary>高级设置</summary>
            <div className="form-group">
              <label>显示名称（可选）</label>
              <input
                value={customSourceForm.display_name}
                onChange={(event) => setCustomSourceForm((form) => ({ ...form, display_name: event.target.value }))}
                placeholder="默认使用识别到的软件名"
                spellCheck={false}
              />
            </div>
            {isGithub ? (
              <div className="form-group">
                <label>Release 文件匹配规则</label>
                <input
                  value={customSourceForm.asset_match}
                  onChange={(event) => setCustomSourceForm((form) => ({ ...form, asset_match: event.target.value }))}
                  placeholder="识别后自动生成"
                  spellCheck={false}
                />
              </div>
            ) : (
              <>
                <div className="form-group">
                  <label>自动匹配规则</label>
                  <input
                    value={customSourceForm.asset_match}
                    onChange={(event) => setCustomSourceForm((form) => ({ ...form, asset_match: event.target.value }))}
                    placeholder="通常无需修改"
                    spellCheck={false}
                  />
                </div>
                <div className="form-group">
                  <label>来源页面</label>
                  <input
                    value={customSourceForm.page_url}
                    onChange={(event) => setCustomSourceForm((form) => ({ ...form, page_url: event.target.value }))}
                    placeholder="默认使用上面的链接"
                    spellCheck={false}
                  />
                </div>
                <div className="form-group form-grid-2">
                  <label>
                    固定显示版本
                    <input
                      value={customSourceForm.version}
                      onChange={(event) => setCustomSourceForm((form) => ({ ...form, version: event.target.value }))}
                      placeholder="留空自动识别"
                      spellCheck={false}
                    />
                  </label>
                  <label>
                    固定保存文件名
                    <input
                      value={customSourceForm.file_name}
                      onChange={(event) => setCustomSourceForm((form) => ({ ...form, file_name: event.target.value }))}
                      placeholder="留空自动识别"
                      spellCheck={false}
                    />
                  </label>
                </div>
              </>
            )}
            <div className="form-group">
              <label>静默安装参数（可选）</label>
              <input
                value={customSourceForm.silent_install_args}
                onChange={(event) => setCustomSourceForm((form) => ({ ...form, silent_install_args: event.target.value }))}
                placeholder="例如：/VERYSILENT /SUPPRESSMSGBOXES"
                spellCheck={false}
              />
              <small>保存后，缓存安装包会显示“安装”；参数会直接传给安装器。</small>
            </div>
          </details>

          <p className="modal-hint">官网链接会在刷新时再次识别；对于 VS Code 这类官方稳定入口，会自动跟随新版安装包。</p>
          <div className="modal-actions">
            <button className="btn" type="button" onClick={closeCustomSourceModal}>取消</button>
            <button className="btn btn-primary" type="submit" disabled={loading || !canSave}>加入软件库</button>
          </div>
        </form>
      </div>
    );
  }

  return (
    <main className="app">
      <div className="window-chrome">
        <div
          className="window-drag"
          data-tauri-drag-region
          onDoubleClick={() => void getCurrentWindow().toggleMaximize()}
        />
        <WindowControls />
      </div>

      <div className="workspace">
        {/*  sidebar nav  */}
        <nav className="sidebar-nav">
          <p className="sidebar-nav-title">软件管家</p>
          {NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`sidebar-nav-item${nav === item.id ? " active" : ""}`}
              onClick={() => setNav(item.id)}
            >
              {item.id === "winget" ? (
                <img className="sidebar-nav-icon sidebar-nav-icon-image" src={wingetIcon} alt="" />
              ) : (
                <span className={`sidebar-nav-icon sidebar-nav-icon-${item.icon}`} aria-hidden="true" />
              )}
              {item.label}
              {item.badge && <span className="sidebar-nav-badge">{item.badge}</span>}
            </button>
          ))}
        </nav>

        {/*  page content  */}
        <section className="main-panel">
          {nav === "library" && renderLibraryPage()}
          {nav === "packages" && renderPackagesPage()}
          {nav === "winget" && (
            <WingetPage
              status={wingetStatus}
              checking={wingetChecking}
              onRefresh={refreshWingetStatus}
              onOpenTerminal={openWingetTerminal}
              onSearch={searchWingetPackages}
            />
          )}
          {nav === "automation" && <AutomationPage software={software} />}
          {nav === "settings" && renderSettingsPage()}
        </section>
      </div>
      {renderCustomSourceModal()}
    </main>
  );
}

export default App;

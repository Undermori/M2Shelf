import { createContext, useContext, useState, useSyncExternalStore, type ReactNode } from "react";
import type { AppSettings } from "../types/media";
import { api } from "./api";
import { errorMessage } from "./format";

interface SettingsSnapshot {
  settings: AppSettings | null;
  saving: boolean;
  savedSequence: number;
  failure: { sequence: number; message: string } | null;
}

/** The App subscriber has already received this localized persistence failure. */
export class SettingsSaveError extends Error {
  constructor(message: string) { super(message); this.name = "M2ShelfError"; }
}

/** One full-settings writer shared by Settings and the caption, surviving page navigation. */
export class SettingsStore {
  private snapshot: SettingsSnapshot = { settings: null, saving: false, savedSequence: 0, failure: null };
  private listeners = new Set<() => void>();
  private persisted: AppSettings | null = null;
  private loading: Promise<AppSettings> | null = null;
  private revision = 0;
  private requested = false;
  private inFlight = false;
  constructor(private transport = { get: api.getSettings, save: api.updateSettings }) {}
  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private publish(update: Partial<SettingsSnapshot>) {
    this.snapshot = { ...this.snapshot, ...update };
    this.listeners.forEach(listener => listener());
  }
  load = (): Promise<AppSettings> => {
    if (this.snapshot.settings) return Promise.resolve(this.snapshot.settings);
    if (!this.loading) this.loading = this.transport.get().then(settings => {
      this.persisted = settings;
      this.publish({ settings });
      return settings;
    }).finally(() => { this.loading = null; });
    return this.loading;
  };
  change = (update: (current: AppSettings) => AppSettings) => {
    if (!this.snapshot.settings) return;
    this.revision += 1;
    this.requested = true;
    this.publish({ settings: update(this.snapshot.settings), saving: true });
    void this.drain();
  };
  /** Onboarding must await its player-path save before finishing setup. */
  save = async (update: (current: AppSettings) => AppSettings): Promise<AppSettings> => {
    await this.load();
    const failureSequence = this.snapshot.failure?.sequence ?? 0;
    return new Promise((resolve, reject) => {
      const unsubscribe = this.subscribe(() => {
        if (this.snapshot.saving) return;
        unsubscribe();
        if ((this.snapshot.failure?.sequence ?? 0) !== failureSequence) reject(new SettingsSaveError(this.snapshot.failure!.message));
        else resolve(this.snapshot.settings!);
      });
      this.change(update);
    });
  };
  flush = async (): Promise<void> => {
    if (!this.snapshot.saving) return;
    const before=this.snapshot.failure?.sequence??0;
    await new Promise<void>((resolve,reject)=>{
      const unsubscribe=this.subscribe(()=>{
        if(this.snapshot.saving)return;
        unsubscribe();
        if((this.snapshot.failure?.sequence??0)!==before)reject(new SettingsSaveError(this.snapshot.failure!.message));else resolve();
      });
    });
  };
  private async drain() {
    if (this.inFlight) return;
    this.inFlight = true;
    let succeeded = false;
    try {
      while (this.requested) {
        this.requested = false;
        const candidate = this.snapshot.settings!;
        const revision = this.revision;
        try {
          const saved = await this.transport.save(candidate);
          this.persisted = saved;
          succeeded = true;
          // A newer optimistic edit already contains this candidate's edits. Do not overwrite it.
          if (revision === this.revision) this.publish({ settings: saved });
        } catch (error) {
          if (revision !== this.revision) continue;
          succeeded = false;
          this.revision += 1;
          this.requested = false;
          this.publish({ settings: this.persisted, failure: {
            sequence: (this.snapshot.failure?.sequence ?? 0) + 1, message: errorMessage(error),
          } });
        }
      }
    } finally {
      this.inFlight = false;
      this.publish({ saving: false, savedSequence: this.snapshot.savedSequence + (succeeded ? 1 : 0) });
    }
  }
}

const SettingsContext = createContext<SettingsStore | null>(null);
export function AppSettingsProvider({ children, store: supplied }: { children: ReactNode; store?: SettingsStore }) {
  const [store] = useState(() => supplied ?? new SettingsStore());
  return <SettingsContext.Provider value={store}>{children}</SettingsContext.Provider>;
}
export function useAppSettings() {
  const store = useContext(SettingsContext);
  if (!store) throw new Error("AppSettingsProvider is required");
  const snapshot = useSyncExternalStore(store.subscribe, store.getSnapshot);
  return { ...snapshot, load: store.load, change: store.change, save: store.save, flush:store.flush };
}

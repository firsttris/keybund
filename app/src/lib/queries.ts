import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { m } from "@/paraglide/messages";
import { api, errorMessage } from "./api";
import type { Fix, ImportProposal, NewProject, Profile, Settings } from "./bindings";
import { ensureHostKey } from "./host-keys";
import { t } from "./i18n";

export const keys = {
  profiles: ["profiles"] as const,
  plan: ["plan"] as const,
  doctor: ["doctor"] as const,
  generated: (id: string) => ["generated", id] as const,
  which: (q: string) => ["which", q] as const,
  keys: ["keys"] as const,
  paths: ["paths"] as const,
  importProposals: ["import-proposals"] as const,
  cleanup: ["cleanup"] as const,
  settings: ["settings"] as const,
};

export function useProfiles() {
  return useQuery({ queryKey: keys.profiles, queryFn: () => api.loadProfiles() });
}

export function useProfile(id: string) {
  const query = useProfiles();
  return { ...query, data: query.data?.profiles.find((p) => p.profile.id === id) };
}

export function usePlan() {
  return useQuery({ queryKey: keys.plan, queryFn: () => api.planChanges() });
}

export function useDoctor() {
  return useQuery({ queryKey: keys.doctor, queryFn: () => api.runDoctor() });
}

export function useGeneratedConfig(id: string) {
  return useQuery({ queryKey: keys.generated(id), queryFn: () => api.generatedConfig(id) });
}

export function useScanKeys() {
  return useQuery({ queryKey: keys.keys, queryFn: () => api.scanKeys() });
}

export function usePaths() {
  return useQuery({ queryKey: keys.paths, queryFn: () => api.paths() });
}

/** Everything that depends on profiles.toml or the written files. */
function useInvalidateAll() {
  const client = useQueryClient();
  return () => client.invalidateQueries();
}

/** Changes are written right away; one toast says so (repeated saves replace it). */
export function toastSaved() {
  toast.success(m.toast_saved(), { id: "saved", description: m.toast_savedDetail() });
}

/** Writes run one after another, so a quick second click never works on outdated data. */
const writes = { id: "keybund-writes" };

/**
 * Changes a key: `change` gets the current state from the backend, not the
 * one shown when the button was rendered, so two quick edits (e.g. removing
 * two folders) both survive.
 */
export function useUpdateProfile(id: string) {
  const invalidate = useInvalidateAll();
  return useMutation({
    scope: writes,
    mutationFn: async (change: (current: Profile) => Profile) => {
      const current = (await api.loadProfiles()).profiles.find((v) => v.profile.id === id)?.profile;
      if (!current) throw new Error(t({ key: "error_profileNotFound", args: { id } }));
      return api.updateProfile(change(current));
    },
    onSuccess: toastSaved,
    onError: (e) => toast.error(errorMessage(e)),
    // Also after an error: the backend may have saved part of it.
    onSettled: invalidate,
  });
}

export function useSetDefault() {
  const invalidate = useInvalidateAll();
  return useMutation({
    scope: writes,
    mutationFn: (id: string | null) => api.setDefaultProfile(id),
    onSuccess: toastSaved,
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: invalidate,
  });
}

export function useCopyPublicKey() {
  return useMutation({
    mutationFn: (id: string) => api.copyPublicKey(id),
    onSuccess: () => toast.success(m.toast_pubkeyCopied()),
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useTestConnection() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, host }: { id: string; host?: string | null }) => {
      await ensureHostKey(id, host ?? null);
      return api.testConnection(id, host ?? null);
    },
    onSettled: () => client.invalidateQueries({ queryKey: keys.profiles }),
  });
}

export function useApplyChanges() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (diffs: string[]) => api.applyChanges(diffs),
    scope: writes,
    onSettled: invalidate,
  });
}

export function useFixDoctor() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (fix: Fix) => api.fixDoctor(fix),
    scope: writes,
    onSettled: invalidate,
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useCreateProject() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: ({ project, openPage }: { project: NewProject; openPage: boolean }) =>
      api.createProject(project, openPage),
    scope: writes,
    onSettled: invalidate,
  });
}

export function useArchiveProfile() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (id: string) => api.archiveProfile(id),
    scope: writes,
    onSettled: invalidate,
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useImportProposals() {
  return useQuery({ queryKey: keys.importProposals, queryFn: () => api.importProposals() });
}

export function useCleanupPreview() {
  return useQuery({ queryKey: keys.cleanup, queryFn: () => api.cleanupPreview() });
}

export function useImportProfiles() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: ({ chosen, removeOld }: { chosen: ImportProposal[]; removeOld: boolean }) =>
      api.importProfiles(chosen, removeOld),
    scope: writes,
    onSettled: invalidate,
  });
}

export function useCleanupApply() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (diffs: string[]) => api.cleanupApply(diffs),
    scope: writes,
    onSettled: invalidate,
  });
}

export function useInstallKey() {
  return useMutation({
    mutationFn: async ({ id, host }: { id: string; host: string }) => {
      await ensureHostKey(id, host);
      return api.installKey(id, host);
    },
  });
}

export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: () => api.settings() });
}

/** Changes some settings on top of the current ones (a stale form never undoes another change). */
export function useUpdateSettings() {
  const client = useQueryClient();
  return useMutation({
    scope: writes,
    mutationFn: async (patch: Partial<Settings>) => api.updateSettings({ ...(await api.settings()), ...patch }),
    // Paths change the defaults of new keys, the switches the generated configs.
    onSettled: () => client.invalidateQueries(),
  });
}

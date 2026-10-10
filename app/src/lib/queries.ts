import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, errorMessage } from "./api";
import type { Fix, ImportProposal, NewProject, Profile } from "./bindings";

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

export function useUpdateProfile() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (profile: Profile) => api.updateProfile(profile),
    onSuccess: invalidate,
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useSetDefault() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (id: string | null) => api.setDefaultProfile(id),
    onSuccess: invalidate,
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useCopyPublicKey() {
  return useMutation({
    mutationFn: (id: string) => api.copyPublicKey(id),
    onSuccess: () => toast.success("Pubkey in der Zwischenablage"),
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useTestConnection() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.testConnection(id),
    onSettled: () => client.invalidateQueries({ queryKey: keys.profiles }),
  });
}

export function useApplyChanges() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (diffs: string[]) => api.applyChanges(diffs),
    onSuccess: invalidate,
  });
}

export function useFixDoctor() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (fix: Fix) => api.fixDoctor(fix),
    onSuccess: invalidate,
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useCreateProject() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: ({ project, openPage }: { project: NewProject; openPage: boolean }) =>
      api.createProject(project, openPage),
    onSuccess: invalidate,
  });
}

export function useArchiveProfile() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (id: string) => api.archiveProfile(id),
    onSuccess: invalidate,
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
    mutationFn: (chosen: ImportProposal[]) => api.importProfiles(chosen),
    onSuccess: invalidate,
  });
}

export function useCleanupApply() {
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (diffs: string[]) => api.cleanupApply(diffs),
    onSuccess: invalidate,
  });
}

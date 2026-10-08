import { useQuery } from "@tanstack/react-query";
import { getPublicInstanceConfig, INSTANCE_CONFIG_QUERY_KEY } from "@/api/instance";

// The instance's public configuration, read once per page load: it only
// changes when the backend restarts, so it is never refetched in the
// background. Shared by every consumer through the query cache.
export function useInstanceConfig() {
  return useQuery({
    queryKey: INSTANCE_CONFIG_QUERY_KEY,
    queryFn: getPublicInstanceConfig,
    staleTime: Infinity,
    retry: false,
  });
}

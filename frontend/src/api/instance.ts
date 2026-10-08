import { api } from "@/lib/api";

export const INSTANCE_CONFIG_QUERY_KEY = ["instance", "config"] as const;

export interface PublicInstanceConfig {
  hide_landing_page: boolean;
  /** `null` when the instance has Turnstile off. */
  turnstile_site_key: string | null;
}

export async function getPublicInstanceConfig(): Promise<PublicInstanceConfig> {
  const { data } = await api.get<PublicInstanceConfig>('/instance/config');
  return data;
}

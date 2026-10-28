export interface SetupConstraints {
  version: number;
  password_min_chars: number;
  rejected_passwords: string[];
  max_inference_steps: number;
  auth_modes: string[];
  allow_insecure_remote: boolean;
}
export interface SetupStatus {
  constraints?: SetupConstraints;
  model: { cache_dir: string };
  server: { port: number };
}
export interface SetupInput {
  usage_mode: string;
  audio_device: string;
  inference_steps: number;
  model_cache_dir: string | null;
  bind: string | null;
  port: number | null;
  auth_mode: string | null;
  admin_username: string;
  admin_password: string;
}
export interface SetupResult {
  api_url: string;
  audio_device: string;
  auth: string;
  api_token?: string;
}

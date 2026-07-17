/** Kimi Code 的默认 config.toml。表单在 Provider settingsConfig 中
 * 以 { config: string } 的 JSON 形式暂存，提交时由后端写回 TOML 文件。 */
export const KIMI_CODE_DEFAULT_TOML = `default_model = "managed:kimi-code/kimi-for-coding"

[providers."managed:kimi-code"]
type = "kimi"
base_url = "https://api.kimi.com/coding/v1"
oauth = { storage = "file", key = "oauth/kimi-code" }

[models."managed:kimi-code/kimi-for-coding"]
provider = "managed:kimi-code"
model = "kimi-for-coding"
max_context_size = 262144
`;

export const KIMI_CODE_DEFAULT_CONFIG = JSON.stringify(
  { config: KIMI_CODE_DEFAULT_TOML },
  null,
  2,
);

export function extractKimiCodeToml(settingsConfig: string): string {
  try {
    const parsed = JSON.parse(settingsConfig) as {
      config?: unknown;
    };
    return typeof parsed.config === "string" ? parsed.config : "";
  } catch {
    return "";
  }
}

export function serializeKimiCodeToml(config: string): string {
  return JSON.stringify({ config }, null, 2);
}

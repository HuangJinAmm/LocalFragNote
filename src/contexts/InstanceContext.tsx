// 本地应用 InstanceContext：从 IPC 加载 instance settings，支持保存
import { create } from "@bufbuild/protobuf";
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import { instanceServiceClient } from "@/connect";
import {
  type InstanceProfile,
  type InstanceSetting,
  type InstanceSetting_GeneralSetting,
  type InstanceSetting_MemoRelatedSetting,
  InstanceProfileSchema,
  InstanceSetting_GeneralSettingSchema,
  InstanceSetting_MemoRelatedSettingSchema,
  InstanceSetting_Key,
} from "@/types/proto/api/v1/instance_service_pb";

interface InstanceState {
  profile: InstanceProfile;
  settings: Record<string, InstanceSetting>;
  isInitialized: boolean;
  isLoading: boolean;
  profileLoaded: boolean;
}

interface InstanceContextValue extends InstanceState {
  generalSetting: InstanceSetting_GeneralSetting;
  memoRelatedSetting: InstanceSetting_MemoRelatedSetting;
  // 本地应用已删除的功能：使用空对象
  storageSetting: Record<string, never>;
  notificationSetting: Record<string, never>;
  aiSetting: { providers: Array<{ id: string; apiKeySet?: boolean }>; transcription?: { providerId?: string } };
  initialize: () => Promise<void>;
  fetchSetting: (key: InstanceSetting_Key) => Promise<void>;
  fetchSettings: (keys: InstanceSetting_Key[]) => Promise<void>;
  updateSetting: (setting: InstanceSetting) => Promise<void>;
}

const InstanceContext = createContext<InstanceContextValue | null>(null);

const LOCAL_PROFILE = create(InstanceProfileSchema, {
  version: "0.1.0-local",
  instanceUrl: "",
  demo: false,
  needsSetup: false,
});

const DEFAULT_GENERAL = create(InstanceSetting_GeneralSettingSchema, {});
const DEFAULT_MEMO_RELATED = create(InstanceSetting_MemoRelatedSettingSchema, {
  reactions: ["👍", "❤️"], // 默认 reactions，避免"反应列表不能为空"报错
});

/// 从 setting 中提取 generalSetting
function extractGeneral(setting: InstanceSetting | undefined): InstanceSetting_GeneralSetting {
  if (setting?.value?.case === "generalSetting" && setting.value.value) {
    return create(InstanceSetting_GeneralSettingSchema, setting.value.value);
  }
  return create(InstanceSetting_GeneralSettingSchema, {});
}

/// 从 setting 中提取 memoRelatedSetting
function extractMemoRelated(setting: InstanceSetting | undefined): InstanceSetting_MemoRelatedSetting {
  if (setting?.value?.case === "memoRelatedSetting" && setting.value.value) {
    return create(InstanceSetting_MemoRelatedSettingSchema, setting.value.value);
  }
  // 默认值，包含默认 reactions
  return create(InstanceSetting_MemoRelatedSettingSchema, {
    reactions: ["👍", "❤️"],
  });
}

/// 构建 instance setting name
function buildSettingName(key: InstanceSetting_Key): string {
  const keyName = InstanceSetting_Key[key];
  return `instance/settings/${keyName}`;
}

export function InstanceProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<InstanceState>({
    profile: LOCAL_PROFILE,
    settings: {},
    isInitialized: true,
    isLoading: false,
    profileLoaded: true,
  });

  const initialize = useCallback(async () => {
    setState((prev) => ({ ...prev, isInitialized: true, profileLoaded: true }));
  }, []);

  /// 加载单个 instance setting
  const fetchSetting = useCallback(async (key: InstanceSetting_Key) => {
    try {
      const name = buildSettingName(key);
      const setting = await instanceServiceClient.getInstanceSetting({ name });
      setState((prev) => ({
        ...prev,
        settings: { ...prev.settings, [key]: setting },
      }));
    } catch {
      // 忽略错误，保持默认值
    }
  }, []);

  /// 批量加载 instance settings
  const fetchSettings = useCallback(async (keys: InstanceSetting_Key[]) => {
    await Promise.all(keys.map((k) => fetchSetting(k)));
  }, [fetchSetting]);

  /// 保存 instance setting
  const updateSetting = useCallback(async (setting: InstanceSetting) => {
    await instanceServiceClient.updateInstanceSetting({ setting });
    // 从 setting.name 反查 key 并缓存
    const keyMatch = setting.name.match(/instance\/settings\/(\w+)/);
    if (keyMatch) {
      const keyName = keyMatch[1] as keyof typeof InstanceSetting_Key;
      const key = InstanceSetting_Key[keyName];
      if (key !== undefined) {
        setState((prev) => ({
          ...prev,
          settings: { ...prev.settings, [key]: setting },
        }));
      }
    }
  }, []);

  const generalSetting = useMemo(
    () => extractGeneral(state.settings[InstanceSetting_Key.GENERAL]),
    [state.settings],
  );
  const memoRelatedSetting = useMemo(
    () => extractMemoRelated(state.settings[InstanceSetting_Key.MEMO_RELATED]),
    [state.settings],
  );

  const value = useMemo(
    () => ({
      ...state,
      generalSetting,
      memoRelatedSetting,
      storageSetting: {},
      notificationSetting: {},
      aiSetting: { providers: [], transcription: { providerId: "" } },
      initialize,
      fetchSetting,
      fetchSettings,
      updateSetting,
    }),
    [state, generalSetting, memoRelatedSetting, initialize, fetchSetting, fetchSettings, updateSetting],
  );

  return <InstanceContext.Provider value={value}>{children}</InstanceContext.Provider>;
}

export function useInstance() {
  const context = useContext(InstanceContext);
  if (!context) {
    throw new Error("useInstance must be used within InstanceProvider");
  }
  return context;
}

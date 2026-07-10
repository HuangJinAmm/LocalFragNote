// 本地应用 AuthContext：无认证，返回固定本地用户，加载持久化的 user settings
import { create } from "@bufbuild/protobuf";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { userServiceClient } from "@/connect";
import {
  type User,
  type UserSetting_GeneralSetting,
  type UserSetting_TagsSetting,
  UserSetting_GeneralSettingSchema,
  UserSetting_TagsSettingSchema,
  UserSchema,
} from "@/types/proto/api/v1/user_service_pb";

// 固定的本地用户（单用户本地应用，无登录认证）
const LOCAL_USER_NAME = "users/local";
const LOCAL_USER = create(UserSchema, {
  name: LOCAL_USER_NAME,
  username: "local",
  displayName: "Local",
  role: 2, // ADMIN（=2），本地单用户应用，所有设置可见
  state: 1, // ACTIVE
});

interface AuthState {
  currentUser: User | undefined;
  userGeneralSetting: UserSetting_GeneralSetting | undefined;
  userWebhooksSetting: undefined;
  userTagsSetting: UserSetting_TagsSetting | undefined;
  // 本地应用已删除 shortcuts 功能，保留字段以兼容现有 hooks
  shortcuts: any[];
  isInitialized: boolean;
  isLoading: boolean;
}

interface AuthContextValue extends AuthState {
  initialize: () => Promise<void>;
  logout: () => Promise<void>;
  refetchSettings: () => Promise<void>;
  setCurrentUser: (user: User | undefined) => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

/// 从 listUserSettings 响应中解析 generalSetting 和 tagsSetting
function parseSettings(settings: any[]): {
  general?: UserSetting_GeneralSetting;
  tags?: UserSetting_TagsSetting;
} {
  const result: { general?: UserSetting_GeneralSetting; tags?: UserSetting_TagsSetting } = {};
  for (const s of settings) {
    // value 是 oneof 对象，形如 { case: "generalSetting", value: {...} }
    if (s?.value?.case === "generalSetting" && s.value.value) {
      result.general = create(UserSetting_GeneralSettingSchema, s.value.value);
    } else if (s?.value?.case === "tagsSetting" && s.value.value) {
      result.tags = create(UserSetting_TagsSettingSchema, s.value.value);
    }
  }
  return result;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({
    currentUser: LOCAL_USER,
    userGeneralSetting: undefined,
    userWebhooksSetting: undefined,
    userTagsSetting: undefined,
    shortcuts: [],
    isInitialized: false,
    isLoading: true,
  });

  /// 从 IPC 加载 user settings
  const loadSettings = useCallback(async () => {
    try {
      const { settings } = await userServiceClient.listUserSettings({ parent: LOCAL_USER_NAME });
      const parsed = parseSettings(settings || []);
      setState((prev) => ({
        ...prev,
        userGeneralSetting: parsed.general,
        userTagsSetting: parsed.tags,
        isInitialized: true,
        isLoading: false,
      }));
    } catch {
      setState((prev) => ({ ...prev, isInitialized: true, isLoading: false }));
    }
  }, []);

  const initialize = useCallback(async () => {
    setState((prev) => ({ ...prev, isLoading: true }));
    await loadSettings();
  }, [loadSettings]);

  const logout = useCallback(async () => {
    // 本地应用无登出
  }, []);

  const refetchSettings = useCallback(async () => {
    await loadSettings();
  }, [loadSettings]);

  const setCurrentUser = useCallback((user: User | undefined) => {
    setState((prev) => ({ ...prev, currentUser: user ?? LOCAL_USER }));
  }, []);

  // 初始化时加载设置
  useEffect(() => {
    void initialize();
  }, [initialize]);

  const value = useMemo(
    () => ({ ...state, initialize, logout, refetchSettings, setCurrentUser }),
    [state, initialize, logout, refetchSettings, setCurrentUser],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return context;
}

export function useCurrentUserFromAuth() {
  const { currentUser } = useAuth();
  return currentUser;
}

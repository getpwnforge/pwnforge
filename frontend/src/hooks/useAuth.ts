// src/hooks/useAuth.ts
import { useContext } from "react";
import { AuthContext, type AuthContextValue } from "@/lib/auth";

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);

  if (!context) {
    throw new Error("useAuth must be used inside <AuthProvider>");
  }

  return context;
}

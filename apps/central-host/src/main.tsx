import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { App } from "@/App"
import { AuthSetupRequired } from "@/auth/AuthSetupRequired"
import { FirebaseAuthProvider } from "@/auth/FirebaseAuthProvider"
import { createFirebaseAuth } from "@/auth/firebase"
import { ChenTheme } from "@chenchess/ui/theme"
import "@chenchess/ui/styles.css"
import "@chenchess/ui/surfaces.css"
import "./styles.css"

const firebaseAuth = createFirebaseAuth(import.meta.env)

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ChenTheme>
      {firebaseAuth ? (
        <FirebaseAuthProvider auth={firebaseAuth}>
          <App />
        </FirebaseAuthProvider>
      ) : (
        <AuthSetupRequired />
      )}
    </ChenTheme>
  </StrictMode>,
)

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { AuthSetupRequired } from "@/auth/AuthSetupRequired"
import { FirebaseAuthProvider } from "@/auth/FirebaseAuthProvider"
import { createFirebaseAuth } from "@/auth/firebase"
import { captureInvitationCode } from "@/auth/invitationFragment"
import { JoinIdentity } from "@/auth/JoinIdentity"
import { verifiedIdentityDestination } from "@/auth/verifiedIdentityDestination"
import { ChenTheme } from "@chenchess/ui/theme"
import "@chenchess/ui/styles.css"
import "@chenchess/ui/surfaces.css"
import "./styles.css"

const initialInvitationCode = captureInvitationCode(
  window.location,
  window.history,
)
const firebaseAuth = createFirebaseAuth(import.meta.env)
const destination = verifiedIdentityDestination(window.location.search)
const navigate = (href: string) => window.location.replace(href)

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ChenTheme>
      {firebaseAuth ? (
        <FirebaseAuthProvider auth={firebaseAuth}>
          <JoinIdentity
            initialInvitationCode={initialInvitationCode}
            navigate={navigate}
            verifiedDestination={destination}
          />
        </FirebaseAuthProvider>
      ) : (
        <AuthSetupRequired />
      )}
    </ChenTheme>
  </StrictMode>,
)

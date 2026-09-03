import { getApps, initializeApp, type FirebaseOptions } from "firebase/app"
import { connectAuthEmulator, getAuth, type Auth } from "firebase/auth"

export type FirebasePublicEnvironment = {
  VITE_FIREBASE_API_KEY?: string
  VITE_FIREBASE_APP_ID?: string
  VITE_FIREBASE_AUTH_DOMAIN?: string
  VITE_FIREBASE_AUTH_EMULATOR_URL?: string
  VITE_FIREBASE_PROJECT_ID?: string
}

let connectedToEmulator: Auth | null = null

export function createFirebaseAuth(
  environment: FirebasePublicEnvironment,
): Auth | null {
  const config = firebaseConfig(environment)
  if (!config) return null
  const app = getApps()[0] ?? initializeApp(config)
  const auth = getAuth(app)
  const emulatorUrl = environment.VITE_FIREBASE_AUTH_EMULATOR_URL?.trim()
  // The SDK throws when connectAuthEmulator runs twice on one Auth, and
  // getAuth hands every entry the same one, so the connection is remembered.
  if (emulatorUrl && connectedToEmulator !== auth) {
    connectAuthEmulator(auth, emulatorUrl, { disableWarnings: true })
    connectedToEmulator = auth
  }
  return auth
}

export function firebaseConfig(
  environment: FirebasePublicEnvironment,
): FirebaseOptions | null {
  const apiKey = environment.VITE_FIREBASE_API_KEY?.trim()
  const appId = environment.VITE_FIREBASE_APP_ID?.trim()
  const authDomain = environment.VITE_FIREBASE_AUTH_DOMAIN?.trim()
  const projectId = environment.VITE_FIREBASE_PROJECT_ID?.trim()
  if (!apiKey || !appId || !authDomain || !projectId) return null
  return { apiKey, appId, authDomain, projectId }
}

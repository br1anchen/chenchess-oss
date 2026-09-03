// @vitest-environment jsdom
import { describe, expect, test } from "vitest"

import { createFirebaseAuth, firebaseConfig } from "./firebase"

describe("Firebase public configuration", () => {
  test("requires the complete browser application identity", () => {
    expect(firebaseConfig({ VITE_FIREBASE_API_KEY: "api-key" })).toBeNull()
    expect(
      firebaseConfig({
        VITE_FIREBASE_API_KEY: "api-key",
        VITE_FIREBASE_APP_ID: "app-id",
        VITE_FIREBASE_AUTH_DOMAIN: "chenchess.firebaseapp.com",
        VITE_FIREBASE_PROJECT_ID: "chenchess",
      }),
    ).toEqual({
      apiKey: "api-key",
      appId: "app-id",
      authDomain: "chenchess.firebaseapp.com",
      projectId: "chenchess",
    })
  })

  test("signs in against the Auth emulator when the local URL is set", () => {
    const environment = {
      VITE_FIREBASE_API_KEY: "emulator-key",
      VITE_FIREBASE_APP_ID: "1:0:web:emulator",
      VITE_FIREBASE_AUTH_DOMAIN: "chenchess-local.firebaseapp.com",
      VITE_FIREBASE_AUTH_EMULATOR_URL: "http://127.0.0.1:9099",
      VITE_FIREBASE_PROJECT_ID: "chenchess-local",
    }

    const auth = createFirebaseAuth(environment)

    expect(auth?.emulatorConfig).toEqual({
      host: "127.0.0.1",
      options: { disableWarnings: true },
      port: 9099,
      protocol: "http",
    })
    // A second entry must not reconnect it: the SDK throws when the same Auth
    // is connected twice.
    expect(createFirebaseAuth(environment)).toBe(auth)
    // An entry without the browser identity still gets the setup-required null.
    expect(createFirebaseAuth({ VITE_FIREBASE_API_KEY: "api-key" })).toBeNull()
  })
})

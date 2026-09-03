import type {
  DocumentData,
  DocumentReference,
  Firestore,
} from "firebase-admin/firestore"

export async function deleteFirestoreDocuments(
  firestore: Firestore,
  references: readonly DocumentReference<DocumentData>[],
  committed: (count: number) => void,
) {
  const pending = [...references]
  if (pending.length === 0) committed(0)
  while (pending.length > 0) {
    const batch = firestore.batch()
    const batchReferences = pending.splice(0, 400)
    for (const reference of batchReferences) batch.delete(reference)
    await batch.commit()
    committed(batchReferences.length)
  }
}

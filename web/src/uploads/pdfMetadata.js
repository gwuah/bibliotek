import { GlobalWorkerOptions, getDocument } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { metadataFromPdf } from "../metadata.js";

GlobalWorkerOptions.workerSrc = workerUrl;

export async function readPdfMetadata(file) {
  const task = getDocument({ data: new Uint8Array(await file.arrayBuffer()) });
  try {
    const pdf = await task.promise;
    const { info } = await pdf.getMetadata();
    return metadataFromPdf(info, file.name);
  } catch {
    return metadataFromPdf({}, file.name);
  } finally {
    await task.destroy();
  }
}

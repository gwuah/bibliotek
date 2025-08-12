import axios from "axios";

async function uploadFileInChunks(file, options = {}) {
  const {
    chunkSize = 1024 * 1024, // 1MB chunks by default
    maxConcurrent = 3,
    onProgress,
    uploadUrl = "/upload",
    resumeUrl = "/upload/resume",
  } = options;

  const totalChunks = Math.ceil(file.size / chunkSize);
  const fileId = generateFileId(); // Generate unique ID for this upload
  let uploadedChunks = 0;

  // Function to upload a single chunk
  const uploadChunk = async (chunk, chunkIndex) => {
    const formData = new FormData();
    formData.append("chunk", chunk);
    formData.append("chunkIndex", chunkIndex);
    formData.append("totalChunks", totalChunks);
    formData.append("fileId", fileId);
    formData.append("fileName", file.name);

    try {
      const response = await axios.post(uploadUrl, formData, {
        headers: {
          "Content-Type": "multipart/form-data",
        },
        onUploadProgress: (progressEvent) => {
          // Calculate progress for this specific chunk
          const chunkProgress = Math.round(
            (progressEvent.loaded * 100) / progressEvent.total
          );

          // Call the overall progress callback if provided
          if (onProgress) {
            const overallProgress = Math.round(
              ((uploadedChunks + chunkProgress / 100) * 100) / totalChunks
            );
            onProgress({
              chunkIndex,
              chunkProgress,
              overallProgress,
              uploadedChunks,
              totalChunks,
            });
          }
        },
      });

      uploadedChunks++;
      return response.data;
    } catch (error) {
      throw new Error(`Failed to upload chunk ${chunkIndex}: ${error.message}`);
    }
  };

  // Create chunks and upload them
  const chunks = [];
  for (let i = 0; i < totalChunks; i++) {
    const start = i * chunkSize;
    const end = Math.min(start + chunkSize, file.size);
    const chunk = file.slice(start, end);
    chunks.push({ chunk, index: i });
  }

  // Upload chunks with concurrency control
  const results = [];
  for (let i = 0; i < chunks.length; i += maxConcurrent) {
    const batch = chunks.slice(i, i + maxConcurrent);
    const batchPromises = batch.map(({ chunk, index }) =>
      uploadChunk(chunk, index)
    );

    const batchResults = await Promise.all(batchPromises);
    results.push(...batchResults);
  }

  return results;
}

// Helper function to generate unique file ID
function generateFileId() {
  return Date.now().toString(36) + Math.random().toString(36).substr(2);
}

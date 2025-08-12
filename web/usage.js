// Usage
const fileInput = document.getElementById('fileInput');
const progressBar = document.getElementById('progressBar');
const statusDiv = document.getElementById('status');

fileInput.addEventListener('change', async (event) => {
  const file = event.target.files[0];
  if (!file) return;

  try {
    await uploadFileInChunks(file, {
      chunkSize: 2 * 1024 * 1024, // 2MB chunks
      maxConcurrent: 2,
      onProgress: (progress) => {
        progressBar.style.width = `${progress.overallProgress}%`;
        statusDiv.textContent = `Uploading chunk ${progress.chunkIndex + 1}/${progress.totalChunks} - ${progress.overallProgress}%`;
      }
    });
    
    statusDiv.textContent = 'Upload completed!';
  } catch (error) {
    statusDiv.textContent = `Upload failed: ${error.message}`;
  }
});
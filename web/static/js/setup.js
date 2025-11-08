class Bibliotek {
  constructor() {
    this.metadata = {};
    this.books = [];
    this.isLoadingMetadata = false;
    this.isLoadingBooks = false;
    this.init();
  }

  renderMetadata() {
    const createMetadataItem = (key, value) => {
      const listItem = document.createElement("li");
      listItem.classList.add("expandable");
      listItem.setAttribute("data-expanded", "false");
      listItem.innerHTML = `
<div>
    <span class="expand-icon"><expand-icon></expand-icon></span>
    <span>${key} [${value}]</span>
</div>`;

      let keyToType = {
        authors: "author",
        tags: "tag",
        categories: "category",
        ratings: "rating",
      };
      const subList = createSubList(this.metadata[key], keyToType[key]);
      listItem.appendChild(subList);
      return listItem;
    };

    const createSubList = (value, type) => {
      const subList = document.createElement("ul");
      subList.classList.add("sub-list", "hidden");

      let subListItems = value.map((item) => {
        const listItem = document.createElement("li");
        listItem.innerHTML = `${item[type].name} (${item.count})`;
        return listItem;
      });
      subListItems.forEach((item) => subList.appendChild(item));
      return subList;
    };

    const aggregatesList = document.getElementById("aggregates-list");
    aggregatesList.innerHTML = "";

    Object.keys(this.metadata).forEach((key) => {
      const listItem = createMetadataItem(key, this.metadata[key].length);
      aggregatesList.appendChild(listItem);
    });
  }

  async initializeEventListeners() {
    const expandableItems = document.querySelectorAll(".expandable");
    expandableItems.forEach((item) => {
      item.addEventListener("click", (e) => {
        e.preventDefault();
        this.toggleExpansion(item);
      });

      const subListItems = item.querySelectorAll(".sub-list li");
      subListItems.forEach((subListItem) => {
        subListItem.addEventListener("click", (e) => {
          e.stopPropagation();
          e.preventDefault();
          console.log("subListItem clicked", subListItem.textContent);
        });
      });
    });
  }

  toggleExpansion(expandableItem) {
    const newState = !(expandableItem.dataset.expanded === "true");

    expandableItem.dataset.expanded = String(newState);

    expandableItem.querySelector(".expand-icon").innerHTML = newState
      ? "<minus-icon></minus-icon>"
      : "<expand-icon></expand-icon>";

    expandableItem.querySelector(".sub-list").style.display = newState
      ? "block"
      : "none";
  }

  async loadMetadata() {
    this.isLoadingMetadata = true;
    let response = await fetch("/metadata");
    let data = await response.json();
    this.metadata = data.metadata;
    this.renderMetadata();
    this.isLoadingMetadata = false;
  }

  async loadBooks() {
    this.isLoadingBooks = true;
    try {
      let response = await fetch("/books");
      if (response.ok) {
        this.books = (await response.json()).books;
      } else {
        console.warn("Could not load books.json, using empty array");
        this.books = [];
      }
    } catch (error) {
      console.warn("Error loading books:", error);
      this.books = [];
    }
    this.isLoadingBooks = false;
    this.renderBooks();
  }

  renderBooks() {
    const booksContainer = document.getElementById("books-container");
    booksContainer.innerHTML = ""; // Clear existing content

    if (this.books.length === 0) {
      booksContainer.innerHTML = "<p>No books found</p>";
      return;
    }

    // Create a grid container for books
    const booksGrid = document.createElement("div");
    booksGrid.classList.add("books-grid");

    this.books.forEach((book) => {
      const bookItem = document.createElement("div");
      bookItem.classList.add("book-item");

      // Create skeleton placeholder
      const skeleton = document.createElement("div");
      skeleton.classList.add("skeleton", "book-cover-skeleton");

      // Create book cover container
      const bookCover = document.createElement("div");
      bookCover.classList.add("book-cover");
      bookCover.appendChild(skeleton);

      // Create image element
      const img = document.createElement("img");
      img.src = book.cover_url;
      img.alt = book.title;
      img.loading = "lazy";
      img.classList.add("loading");

      // Handle image load success
      img.onload = () => {
        // img.classList.add("book-cover-img");
        img.classList.add("loaded");
        img.classList.remove("loading");
        skeleton.remove();
      };

      // Handle image load error
      img.onerror = () => {
        img.src =
          "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTAwIiBoZWlnaHQ9IjE1MCIgdmlld0JveD0iMCAwIDEwMCAxNTAiIGZpbGw9Im5vbmUiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+CjxyZWN0IHdpZHRoPSIxMDAiIGhlaWdodD0iMTUwIiBmaWxsPSIjZjNmNGY2Ii8+CjxwYXRoIGQ9Ik00NSA2MEw1NSA2MEw1NSA3MEw0NSA3MFoiIGZpbGw9IiNkMWQ1ZGIiLz4KPHN2Zz4K";
        img.classList.remove("loading");
        img.classList.add("loaded");
        // img.classList.add("book-cover-img");
        skeleton.remove();
      };

      bookCover.appendChild(img);

      const bookInfo = document.createElement("div");
      bookInfo.classList.add("book-info");
      bookInfo.innerHTML = `
        <h3 class="book-title">${book.title}</h3>
        <p class="book-pages">${book.pages} pages</p>
      `;

      // Assemble book item
      bookItem.appendChild(bookCover);
      bookItem.appendChild(bookInfo);
      booksGrid.appendChild(bookItem);
    });

    booksContainer.appendChild(booksGrid);
  }

  async init() {
    await this.loadMetadata();
    await this.loadBooks();
    // await this.renderMetadata();
    // await this.renderBooks();
    await this.initializeEventListeners();
  }
}

class MassUploader {
  constructor(afterUpload) {
    this.afterUpload = afterUpload;
    this.queue = [];
    this.maxConcurrent = 5;
    this.activeUploads = 0;
    this.isRunning = false;
    this.elements = {
      dropArea: document.getElementById("file-drop-area"),
      fileInput: document.getElementById("dropzone-file"),
      queueList: document.getElementById("upload-queue"),
      controls: document.getElementById("upload-controls"),
      startButton: document.getElementById("start-upload-button"),
      summary: document.getElementById("upload-summary"),
    };
    this.init();
  }

  async init() {
    this.bindEvents();
    this.updateControlsVisibility();
    this.updateSummary();
  }

  bindEvents() {
    const { dropArea, fileInput, startButton } = this.elements;

    dropArea.addEventListener("click", () => {
      fileInput.click();
    });

    fileInput.addEventListener("change", (event) => {
      const files = Array.from(event.target.files || []);
      this.handleIncomingFiles(files);
      fileInput.value = "";
    });

    ["dragenter", "dragover"].forEach((eventName) => {
      dropArea.addEventListener(eventName, (event) => {
        event.preventDefault();
        dropArea.classList.add("dragging");
      });
    });

    ["dragleave", "drop"].forEach((eventName) => {
      dropArea.addEventListener(eventName, (event) => {
        event.preventDefault();
        dropArea.classList.remove("dragging");
      });
    });

    dropArea.addEventListener("drop", (event) => {
      const files = Array.from(event.dataTransfer?.files || []);
      this.handleIncomingFiles(files);
    });

    document.addEventListener("paste", (event) => {
      const files = Array.from(event.clipboardData?.files || []);
      if (files.length > 0) {
        this.handleIncomingFiles(files);
      }
    });

    startButton.addEventListener("click", () => {
      this.startUploads();
    });
  }

  handleIncomingFiles(files) {
    if (!files || files.length === 0) {
      return;
    }

    const normalized = files.filter((file) => {
      const isPdf =
        file.type === "application/pdf" ||
        file.name.toLowerCase().endsWith(".pdf");
      return isPdf;
    });

    const newEntries = normalized.filter((file) => {
      const signature = this.signatureFor(file);
      return !this.queue.some((entry) => entry.signature === signature);
    });

    if (newEntries.length === 0) {
      return;
    }

    newEntries.forEach((file) => {
      const entry = this.createQueueEntry(file);
      this.queue.push(entry);
      this.elements.queueList.appendChild(entry.element);
    });

    this.updateControlsVisibility();
    this.updateSummary();

    if (this.isRunning) {
      this.processQueue();
    }
  }

  signatureFor(file) {
    return `${file.name}-${file.size}-${file.lastModified}`;
  }

  createQueueEntry(file) {
    const element = document.createElement("li");
    element.classList.add("upload-item");

    const header = document.createElement("div");
    header.classList.add("upload-item-header");

    const name = document.createElement("span");
    name.classList.add("upload-item-name");
    name.textContent = file.name;

    const status = document.createElement("span");
    status.classList.add("upload-item-status");
    status.textContent = "pending";

    header.appendChild(name);
    header.appendChild(status);

    const track = document.createElement("div");
    track.classList.add("upload-progress-track");

    const fill = document.createElement("div");
    fill.classList.add("upload-progress-fill", "pending");
    fill.style.width = "0%";

    track.appendChild(fill);
    element.appendChild(header);
    element.appendChild(track);

    return {
      id: crypto.randomUUID ? crypto.randomUUID() : `upload-${Date.now()}-${Math.random()}`,
      file,
      status: "pending",
      progress: 0,
      uploadId: null,
      element,
      statusEl: status,
      progressEl: fill,
      signature: this.signatureFor(file),
    };
  }

  updateControlsVisibility() {
    const hasQueue = this.queue.length > 0;
    const { controls } = this.elements;
    controls.classList.toggle("hidden", !hasQueue);
    this.updateStartButtonState();
  }

  updateSummary() {
    const { summary } = this.elements;
    const total = this.queue.length;
    const completed = this.queue.filter((entry) => entry.status === "completed")
      .length;
    summary.textContent = `${completed}/${total} completed`;
  }

  updateStartButtonState() {
    const { startButton } = this.elements;
    const hasPending = this.queue.some((entry) => entry.status === "pending");
    const hasErrors = this.queue.some((entry) => entry.status === "error");
    if (this.isRunning) {
      startButton.textContent = "uploading...";
      startButton.classList.add("disabled");
      startButton.disabled = true;
      return;
    }

    startButton.textContent = hasErrors && !hasPending ? "retry failed uploads" : "start upload";
    const enable = hasPending || hasErrors;
    startButton.disabled = !enable;
    startButton.classList.toggle("disabled", !enable);
  }

  startUploads() {
    if (this.isRunning) {
      return;
    }

    let hasPending = this.queue.some((entry) => entry.status === "pending");

    if (!hasPending) {
      const hasErrors = this.queue.some((entry) => entry.status === "error");
      if (hasErrors) {
        this.queue.forEach((entry) => {
          if (entry.status === "error") {
            this.resetEntryToPending(entry);
          }
        });
        hasPending = this.queue.some((entry) => entry.status === "pending");
      }
    }

    if (!hasPending) {
      return;
    }

    this.isRunning = true;
    this.updateStartButtonState();
    this.processQueue();
  }

  processQueue() {
    if (!this.isRunning) {
      return;
    }

    while (this.activeUploads < this.maxConcurrent) {
      const nextEntry = this.queue.find((entry) => entry.status === "pending");
      if (!nextEntry) {
        break;
      }
      this.uploadEntry(nextEntry);
    }

    if (
      this.activeUploads === 0 &&
      this.queue.every(
        (entry) => entry.status === "completed" || entry.status === "error"
      )
    ) {
      this.finishRun();
    }
  }

  async uploadEntry(entry) {
    this.activeUploads += 1;
    this.setEntryStatus(entry, "uploading");
    this.setEntryProgress(entry, 0);

    const file = entry.file;
    const chunkSize = 1 * 1024 * 1024;
    const totalChunks = Math.max(1, Math.ceil(file.size / chunkSize));

    try {
      const initResponse = await this.initUpload(file.name);
      entry.uploadId = initResponse.upload_id;

      for (let index = 0; index < totalChunks; index++) {
        const chunk = file.slice(index * chunkSize, (index + 1) * chunkSize);
        await this.uploadChunk(entry.uploadId, chunk, index + 1);
        const progress = Math.round(((index + 1) / totalChunks) * 100);
        this.setEntryProgress(entry, progress);
      }

      await this.completeUpload(entry.uploadId);
      this.markEntryCompleted(entry);
      this.afterUpload();
    } catch (error) {
      console.error(`Error uploading ${file.name}`, error);
      this.markEntryError(entry, error);
    } finally {
      this.activeUploads -= 1;
      this.updateSummary();
      this.processQueue();
    }
  }

  setEntryStatus(entry, status) {
    entry.status = status;
    entry.statusEl.textContent = status;

    entry.element.classList.remove("pending", "uploading", "completed", "error");
    entry.element.classList.add(status);
  }

  setEntryProgress(entry, percentage) {
    entry.progress = percentage;
    entry.progressEl.classList.remove("pending");
    entry.progressEl.style.width = `${percentage}%`;
  }

  markEntryCompleted(entry) {
    this.setEntryStatus(entry, "completed");
    this.setEntryProgress(entry, 100);
  }

  markEntryError(entry) {
    entry.status = "error";
    entry.statusEl.textContent = "error";
    entry.element.classList.remove("pending", "uploading", "completed");
    entry.element.classList.add("error");
  }

  resetEntryToPending(entry) {
    entry.status = "pending";
    entry.statusEl.textContent = "pending";
    entry.element.classList.remove("uploading", "completed", "error");
    entry.progressEl.classList.add("pending");
    entry.progressEl.style.width = "0%";
    entry.progress = 0;
  }

  finishRun() {
    this.isRunning = false;
    this.updateStartButtonState();
    this.updateSummary();
  }

  async initUpload(fileName) {
    const formData = new FormData();
    formData.append("file_name", fileName);
    const response = await axios.post("/upload?state=init", formData);
    return response.data;
  }

  async uploadChunk(uploadId, chunk, partNumber) {
    const formData = new FormData();
    formData.append("chunk", chunk);
    formData.append("upload_id", uploadId);
    formData.append("part_number", partNumber);
    await axios.post("/upload?state=continue", formData);
  }

  async completeUpload(uploadId) {
    const formData = new FormData();
    formData.append("upload_id", uploadId);
    await axios.post("/upload?state=complete", formData);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  window.app = new Bibliotek();
  const afterUpload = () => {
    window.app.loadMetadata();
    window.app.loadBooks();
  };
  window.uploader = new MassUploader(afterUpload);
});

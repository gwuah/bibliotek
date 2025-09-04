class Bibliotek {
  constructor() {
    this.metadata = {};
    this.books = [];
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
    let response = await fetch("/metadata");
    let data = await response.json();
    this.metadata = data.metadata;
  }

  async loadBooks() {
    try {
      let response = await fetch("/static/books.json");
      if (response.ok) {
        this.books = await response.json();
      } else {
        console.warn("Could not load books.json, using empty array");
        this.books = [];
      }
    } catch (error) {
      console.warn("Error loading books:", error);
      this.books = [];
    }
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

  attachFileUploadListener(event) {
    const fileInput = document.getElementById("dropzone-file");
    fileInput.addEventListener("change", (event) => {
      console.log("file", event);
      const file = event.target.files[0];
      document.getElementById("dropzone-file-message").textContent = file.name;
      document.getElementById("upload-button").classList.remove("hidden");
    });
  }

  async init() {
    await this.loadMetadata();
    await this.loadBooks();
    await this.renderMetadata();
    await this.renderBooks();
    await this.initializeEventListeners();
    await this.attachFileUploadListener();
  }
}

class Uploader {
  constructor() {
    this.init();
  }

  freezeUploadFunctionality() {
    let uploadButton = document.getElementById("upload-button");
    let dropzoneFile = document.getElementById("dropzone-file");
    uploadButton.textContent = "uploading...";
    uploadButton.classList.add("disabled");
    uploadButton.disabled = true;
    dropzoneFile.disabled = true;
  }

  resetUploadFunctionality() {
    let uploadButton = document.getElementById("upload-button");
    let dropzoneFile = document.getElementById("dropzone-file");
    let dropzoneFileMessage = document.getElementById("dropzone-file-message");
    dropzoneFile.value = "";
    dropzoneFile.disabled = false;
    uploadButton.disabled = false;
    uploadButton.textContent = "upload";
    uploadButton.classList.remove("disabled");
    uploadButton.classList.add("hidden");
    dropzoneFileMessage.textContent = "click to upload";
  }

  async attachFileUploadListener() {
    const fileInput = document.getElementById("upload-button");
    fileInput.addEventListener("click", (event) => {
      const fileInput = document.getElementById("dropzone-file");
      const file = fileInput.files[0];
      this.uploadFile(file, (progress) => {
        console.log("progress", progress);
      });
    });
  }

  async uploadFile(file, onProgress) {
    this.freezeUploadFunctionality();

    const chunkSize = 1 * 1024 * 1024; // 1MB chunks
    const totalChunks = Math.ceil(file.size / chunkSize);
    const initResponse = await this.initUpload(file.name);
    for (let i = 0; i < totalChunks; i++) {
      const chunk = file.slice(i * chunkSize, (i + 1) * chunkSize);

      try {
        await this.uploadChunk(
          initResponse.upload_id,
          chunk,
          i + 1,
          onProgress
        );
      } catch (error) {
        console.error("Error uploading chunk", error);
        // this.resetUploadFunctionality();
        return;
      }
    }

    await this.completeUpload(initResponse.upload_id);

    this.resetUploadFunctionality();
  }

  async initUpload(fileName) {
    const formData = new FormData();
    formData.append("file_name", fileName);
    const response = await axios.post("/upload?state=init", formData);
    return response.data;
  }

  async uploadChunk(uploadId, chunk, partNumber, onProgress) {
    const formData = new FormData();
    formData.append("chunk", chunk);
    formData.append("upload_id", uploadId);
    formData.append("part_number", partNumber);
    await axios.post("/upload?state=continue", formData, {
      onUploadProgress: (e) => {
        onProgress(e);
      },
    });
  }

  async completeUpload(uploadId) {
    const formData = new FormData();
    formData.append("upload_id", uploadId);
    await axios.post("/upload?state=complete", formData);
  }

  async init() {
    await this.attachFileUploadListener();
  }
}

document.addEventListener("DOMContentLoaded", () => {
  window.app = new Bibliotek();
  window.uploader = new Uploader();
});

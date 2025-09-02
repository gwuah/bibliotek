class BibliotekApp {
    constructor() {
        this.currentPage = 1;
        this.searchQuery = '';
        this.booksPerPage = 20;
        this.metadata = null;
        
        this.initEventListeners();
        this.loadMetadata();
        this.loadBooks();
    }

    initEventListeners() {
        // Search functionality
        const searchInput = document.getElementById('searchInput');
        const searchBtn = document.getElementById('searchBtn');
        
        searchBtn.addEventListener('click', () => this.search());
        searchInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') this.search();
        });

        // Upload functionality
        const uploadBtn = document.getElementById('uploadBtn');
        const fileInput = document.getElementById('fileInput');
        
        uploadBtn.addEventListener('click', () => fileInput.click());
        fileInput.addEventListener('change', (e) => this.handleFileUpload(e));

        // Tree view toggles
        document.addEventListener('click', (e) => {
            if (e.target.classList.contains('tree-toggle')) {
                this.toggleTreeSection(e.target);
            }
        });
    }

    async loadMetadata() {
        try {
            const response = await axios.get('/metadata');
            this.metadata = response.data.metadata;
            this.renderMetadata();
        } catch (error) {
            console.error('Failed to load metadata:', error);
        }
    }

    renderMetadata() {
        if (!this.metadata) return;

        // Render authors
        const authorsCount = document.getElementById('authors-count');
        const authorsList = document.getElementById('authors-list');
        authorsCount.textContent = `(${this.metadata.authors.length})`;
        
        authorsList.innerHTML = this.metadata.authors.map(authorAgg => 
            `<div class="tree-child" data-filter="author" data-value="${authorAgg.author.name}">
                <span>${authorAgg.author.name}</span>
                <span class="count">${authorAgg.book_count}</span>
            </div>`
        ).join('');

        // Render tags
        const tagsCount = document.getElementById('tags-count');
        const tagsList = document.getElementById('tags-list');
        tagsCount.textContent = `(${this.metadata.tags.length})`;
        
        tagsList.innerHTML = this.metadata.tags.map(tagAgg => 
            `<div class="tree-child" data-filter="tag" data-value="${tagAgg.tag.name}">
                <span>${tagAgg.tag.name}</span>
                <span class="count">${tagAgg.book_count}</span>
            </div>`
        ).join('');

        // Render ratings
        const ratingsCount = document.getElementById('ratings-count');
        const ratingsList = document.getElementById('ratings-list');
        ratingsCount.textContent = `(${this.metadata.ratings.length})`;
        
        ratingsList.innerHTML = this.metadata.ratings.map(ratingAgg => 
            `<div class="tree-child" data-filter="rating" data-value="${ratingAgg.rating}">
                <span>${ratingAgg.rating} ⭐</span>
                <span class="count">${ratingAgg.book_count}</span>
            </div>`
        ).join('');

        // Add click handlers for filters
        document.querySelectorAll('.tree-child').forEach(child => {
            child.addEventListener('click', (e) => {
                e.stopPropagation();
                const filterType = child.dataset.filter;
                const filterValue = child.dataset.value;
                this.applyFilter(filterType, filterValue);
            });
        });
    }

    toggleTreeSection(button) {
        const targetId = button.dataset.target;
        const target = document.getElementById(targetId);
        
        button.classList.toggle('open');
        target.classList.toggle('open');
    }

    async loadBooks() {
        try {
            const params = new URLSearchParams({
                page: this.currentPage,
                limit: this.booksPerPage
            });
            
            if (this.searchQuery) {
                params.append('q', this.searchQuery);
            }

            const response = await axios.get(`/books?${params}`);
            this.renderBooks(response.data.books);
            this.renderPagination();
        } catch (error) {
            console.error('Failed to load books:', error);
            this.showError('Failed to load books');
        }
    }

    renderBooks(books) {
        const grid = document.getElementById('booksGrid');
        
        if (books.length === 0) {
            grid.innerHTML = '<div class="loading">No books found</div>';
            return;
        }

        grid.innerHTML = books.map(book => this.createBookCard(book)).join('');
    }

    createBookCard(book) {
        const author = book.author ? book.author.name : 'Unknown Author';
        const rating = book.ratings ? '⭐'.repeat(book.ratings) : 'Not rated';
        const tags = book.tags.map(tag => 
            `<span class="tag">${tag.name}</span>`
        ).join('');

        return `
            <div class="book-card">
                <div class="book-title">${this.escapeHtml(book.title)}</div>
                <div class="book-meta">By ${this.escapeHtml(author)}</div>
                <div class="book-meta">${rating}</div>
                <div class="book-tags">${tags}</div>
                <button class="download-btn" onclick="window.open('${book.download_url}', '_blank')">
                    Download
                </button>
            </div>
        `;
    }

    renderPagination() {
        const pagination = document.getElementById('pagination');
        const totalPages = Math.max(1, Math.ceil(100 / this.booksPerPage)); // Estimate
        
        let paginationHTML = '';
        
        // Previous button
        if (this.currentPage > 1) {
            paginationHTML += `<button class="page-btn" onclick="app.goToPage(${this.currentPage - 1})">‹ Previous</button>`;
        }
        
        // Page numbers
        const startPage = Math.max(1, this.currentPage - 2);
        const endPage = Math.min(totalPages, this.currentPage + 2);
        
        for (let i = startPage; i <= endPage; i++) {
            const isActive = i === this.currentPage ? 'active' : '';
            paginationHTML += `<button class="page-btn ${isActive}" onclick="app.goToPage(${i})">${i}</button>`;
        }
        
        // Next button
        if (this.currentPage < totalPages) {
            paginationHTML += `<button class="page-btn" onclick="app.goToPage(${this.currentPage + 1})">Next ›</button>`;
        }
        
        pagination.innerHTML = paginationHTML;
    }

    goToPage(page) {
        this.currentPage = page;
        this.loadBooks();
    }

    search() {
        const searchInput = document.getElementById('searchInput');
        this.searchQuery = searchInput.value.trim();
        this.currentPage = 1;
        this.loadBooks();
    }

    applyFilter(filterType, filterValue) {
        // For now, just set the search query to the filter value
        // In a more sophisticated implementation, you'd modify the backend to support filters
        const searchInput = document.getElementById('searchInput');
        searchInput.value = filterValue;
        this.search();
    }

    async handleFileUpload(event) {
        const files = Array.from(event.target.files);
        if (files.length === 0) return;

        const progressContainer = document.getElementById('uploadProgress');
        const progressFill = document.getElementById('progressFill');
        const statusMessage = document.getElementById('statusMessage');

        progressContainer.style.display = 'block';

        try {
            for (let i = 0; i < files.length; i++) {
                const file = files[i];
                await this.uploadFile(file, (progress) => {
                    const overallProgress = ((i + progress.overallProgress / 100) / files.length) * 100;
                    progressFill.style.width = `${overallProgress}%`;
                    statusMessage.textContent = `Uploading ${file.name}... ${Math.round(progress.overallProgress)}%`;
                    statusMessage.className = 'status-message';
                    statusMessage.style.display = 'block';
                });
            }

            this.showSuccess('All files uploaded successfully!');
            this.loadBooks(); // Refresh the book list
            this.loadMetadata(); // Refresh metadata
            
            // Clear file input
            event.target.value = '';
            
        } catch (error) {
            console.error('Upload failed:', error);
            this.showError('Upload failed: ' + error.message);
        } finally {
            setTimeout(() => {
                progressContainer.style.display = 'none';
            }, 3000);
        }
    }

    async uploadFile(file, onProgress) {
        const chunkSize = 2 * 1024 * 1024; // 2MB chunks
        const totalChunks = Math.ceil(file.size / chunkSize);
        
        // Initialize upload
        const uploadId = await this.initUpload(file.name);
        
        // Upload chunks
        let uploadedChunks = 0;
        const progressBytes = new Array(totalChunks).fill(0);
        const totalBytes = file.size;

        for (let i = 0; i < totalChunks; i++) {
            const start = i * chunkSize;
            const end = Math.min(start + chunkSize, file.size);
            const chunk = file.slice(start, end);
            
            await this.uploadChunk(uploadId, chunk, (loaded) => {
                progressBytes[i] = loaded;
                const uploadedBytes = progressBytes.reduce((a, b) => a + b, 0);
                const overallProgress = Math.round((uploadedBytes / totalBytes) * 100);
                
                onProgress({
                    chunkIndex: i,
                    totalChunks,
                    overallProgress,
                    uploadedBytes,
                    totalBytes
                });
            });
            
            uploadedChunks++;
        }

        // Complete upload
        await this.completeUpload(uploadId);
        
        return uploadId;
    }

    async initUpload(fileName) {
        const formData = new FormData();
        formData.append('file_name', fileName);
        
        const response = await axios.post('/upload?state=init', formData);
        return response.data.upload_id;
    }

    async uploadChunk(uploadId, chunk, onProgress) {
        const formData = new FormData();
        formData.append('chunk', chunk);
        formData.append('upload_id', uploadId);
        
        await axios.post('/upload?state=continue', formData, {
            onUploadProgress: (e) => {
                onProgress(e.loaded);
            }
        });
    }

    async completeUpload(uploadId) {
        const formData = new FormData();
        formData.append('upload_id', uploadId);
        
        await axios.post('/upload?state=complete', formData);
    }

    showSuccess(message) {
        const statusMessage = document.getElementById('statusMessage');
        statusMessage.textContent = message;
        statusMessage.className = 'status-message success';
        statusMessage.style.display = 'block';
    }

    showError(message) {
        const statusMessage = document.getElementById('statusMessage');
        statusMessage.textContent = message;
        statusMessage.className = 'status-message error';
        statusMessage.style.display = 'block';
    }

    escapeHtml(text) {
        const map = {
            '&': '&amp;',
            '<': '&lt;',
            '>': '&gt;',
            '"': '&quot;',
            "'": '&#039;'
        };
        return text.replace(/[&<>"']/g, (m) => map[m]);
    }
}

// Initialize the app when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    window.app = new BibliotekApp();
});
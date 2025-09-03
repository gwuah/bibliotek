class Bibliotek {
    constructor() {
        this.metadata = [];
        // this.initEventListeners();
        this.loadMetadata();
        
    }
    
    loadMetadata() {
        fetch('/metadata')
            .then(response => response.json())
            .then(data => this.metadata = data);
    }
    
    
}

document.addEventListener('DOMContentLoaded', () => {
    window.app = new Bibliotek();
});
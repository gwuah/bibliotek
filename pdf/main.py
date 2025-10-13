import pymupdf
from pathlib import Path
from datetime import datetime

def parse_pdf_date(pdf_date):
    """Convert PDF date format to readable date"""
    if not pdf_date or not pdf_date.startswith('D:'):
        return None
    try:
        date_part = pdf_date[2:16]
        dt = datetime.strptime(date_part, '%Y%m%d%H%M%S')
        return dt.strftime('%Y-%m-%d %H:%M:%S')
    except:
        return pdf_date

def extract_embedded_metadata(pdf_path):
    """Extract only embedded metadata from a PDF file"""
    try:
        doc = pymupdf.open(pdf_path)
        metadata = doc.metadata
        doc.close()
        
        result = {
            'filename': Path(pdf_path).name,
            'title': metadata.get('title') or None,
            'author': metadata.get('author') or None,
            'subject': metadata.get('subject') or None,
            'keywords': metadata.get('keywords') or None,
            'creator': metadata.get('creator') or None,
            'producer': metadata.get('producer') or None,
            'creation_date': parse_pdf_date(metadata.get('creationDate')),
            'modification_date': parse_pdf_date(metadata.get('modDate')),
        }
        
        return result
        
    except Exception as e:
        print(f"Error reading {pdf_path}: {e}")
        return None

def check_metadata_availability(pdf_path):
    """Check what metadata is available"""
    doc = pymupdf.open(pdf_path)
    metadata = doc.metadata
    doc.close()
    
    print(f"\nChecking: {pdf_path}")
    print("-" * 50)
    
    has_metadata = False
    fields = ['title', 'author', 'subject', 'keywords', 
              'creator', 'producer', 'creationDate', 'modDate']
    
    for field in fields:
        value = metadata.get(field)
        status = "✓" if value else "✗"
        print(f"{status} {field:20s}: {value if value else '(empty)'}")
        if value:
            has_metadata = True
    
    if not has_metadata:
        print("\n⚠️  This PDF has NO embedded metadata!")
    
    return has_metadata

if __name__ == "__main__":
    print("Starting PDF metadata extraction...")
    # Test with a single PDF
    pdf_path = "./pdfs/max-weber-protestant-work-ethics.pdf"  # Change this
    
    # Check what's available
    check_metadata_availability(pdf_path)
    
    # Extract metadata
    metadata = extract_embedded_metadata(pdf_path)
    print(metadata)
    
    if metadata:
        print("\n" + "="*50)
        print("Extracted Metadata:")
        print("="*50)
        for key, value in metadata.items():
            if value:
                print(f"{key:20s}: {value}")

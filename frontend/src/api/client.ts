import type {
  ListFilesResponse,
  FileMetadataResponse,
  FileContentResponse,
  WriteFileRequest,
  WriteFileResponse,
} from '../types/api';

const API_BASE = '/api';

export class ApiClient {
  async listFiles(path: string): Promise<ListFilesResponse> {
    const response = await fetch(`${API_BASE}/files?path=${encodeURIComponent(path)}`);
    if (!response.ok) {
      throw new Error(`Failed to list files: ${response.statusText}`);
    }
    return response.json();
  }

  async getFileMetadata(path: string): Promise<FileMetadataResponse> {
    const response = await fetch(`${API_BASE}/file/metadata?path=${encodeURIComponent(path)}`);
    if (!response.ok) {
      throw new Error(`Failed to get file metadata: ${response.statusText}`);
    }
    return response.json();
  }

  async readFile(path: string): Promise<FileContentResponse> {
    const response = await fetch(`${API_BASE}/file/content?path=${encodeURIComponent(path)}`);
    if (!response.ok) {
      throw new Error(`Failed to read file: ${response.statusText}`);
    }
    return response.json();
  }

  async writeFile(request: WriteFileRequest): Promise<WriteFileResponse> {
    const response = await fetch(`${API_BASE}/file/content`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(request),
    });
    if (!response.ok) {
      throw new Error(`Failed to write file: ${response.statusText}`);
    }
    return response.json();
  }
}

export const apiClient = new ApiClient();

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string | null;
}

export interface ListFilesResponse {
  entries: FileEntry[];
}

export interface FileMetadataResponse {
  path: string;
  is_dir: boolean;
  size: number;
  modified: string | null;
  permissions: number | null;
}

export interface FileContentResponse {
  content: string;
  size: number;
}

export interface WriteFileRequest {
  path: string;
  content: string;
}

export interface WriteFileResponse {
  success: boolean;
  message: string;
}

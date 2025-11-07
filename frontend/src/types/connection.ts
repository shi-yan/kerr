export interface Connection {
  connection_string: string;
  registered_at: number;
  alias: string | null;
  host_name: string;
}

export interface ConnectionsListResponse {
  connections: Connection[];
  count: number;
}

export interface ConnectionStatusResponse {
  connected: boolean;
}

export interface ConnectRequest {
  connection_string: string;
}

export interface ConnectResponse {
  success: boolean;
  message: string;
}

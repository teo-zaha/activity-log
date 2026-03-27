export type ExportFormat = 'json' | 'markdown'

export interface ExportRequest {
  content: string
  fileName: string
  format: ExportFormat
}

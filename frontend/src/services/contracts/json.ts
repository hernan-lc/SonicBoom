export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
export interface NumericBounds {
  min: number;
  max: number;
  integer?: boolean;
  exclusive_min?: boolean;
}

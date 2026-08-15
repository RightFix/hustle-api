from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    project_name: str = "Hustle"
    api_prefix: str = "/api"

    database_url: str = "postgresql+psycopg://hustle:hustle@localhost:5432/hustle"
    secret_key: str = "change-me-in-production"
    access_token_expire_minutes: int = 60 * 24
    algorithm: str = "HS256"

    cors_origin: str = "http://localhost:5173"


settings = Settings()

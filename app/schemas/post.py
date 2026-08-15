from datetime import datetime

from pydantic import BaseModel, ConfigDict


class PostCreate(BaseModel):
    content: str


class PostOut(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: int
    author_id: int
    content: str
    created_at: datetime

    author: "UserPublic | None" = None


class UserPublic(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: int
    username: str
    bio: str | None = None


PostOut.model_rebuild()
